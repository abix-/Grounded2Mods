// G2's WBP_InventoryInterface_C viewport binder. The universal
// scroll/page/rebind algorithm + the ProcessEvent hook trampoline
// + per-widget viewport-start state all live in
// `ueforge::inventory::viewport`. This file owns only the
// Maine-specific knowledge:
//
// - The widget class name + grid offset + page geometry
// - The UFunction parm shapes (`#[repr(C)]` mirrors of the SDK)
// - The "get item at absolute index" path: GetInventoryItems +
//   BPF_InventoryFunctions_C.GetItemInItemListSlot
// - The "initialize visible slot" path: InitializeItemSlot
// - The mouse-wheel-delta extraction:
//   KismetInputLibrary.PointerEvent_GetWheelDelta

use std::ffi::c_void;
use std::sync::atomic::{AtomicI32, Ordering};

use ueforge::hook::ProcessEventHook;
use ueforge::inventory::viewport::{ViewportBinder, ViewportConfig, ViewportHook};
use ueforge::ue::{self, ClassRef, UFunction, UObject};

use crate::parms::{
    F_POINTER_EVENT_SIZE, GetInventoryItemsParms, GetItemInItemListSlotParms,
    InitializeItemSlotParms, OnMouseWheelInputView, PointerEventGetWheelDeltaParms,
};

const VIEWPORT_ROWS: i32 = 4;
const VIEWPORT_COLUMNS: i32 = 10;
const VIEWPORT_PAGE_SIZE: i32 = VIEWPORT_ROWS * VIEWPORT_COLUMNS;
const SCROLL_STEP_SLOTS: i32 = VIEWPORT_COLUMNS;
const INV_IFACE_ITEM_GRID_OFFSET: usize = 0x0430;

const CONFIG: ViewportConfig = ViewportConfig {
    widget_class: "WBP_InventoryInterface_C",
    item_grid_offset: INV_IFACE_ITEM_GRID_OFFSET,
    page_size: VIEWPORT_PAGE_SIZE,
    scroll_step: SCROLL_STEP_SLOTS,
    construct_fn: "Construct",
    on_mouse_wheel_fn: "OnMouseWheel",
    refresh_fns: &[
        "PopulateItemGrid",
        "OnInventoryChanged",
        "OnInventoryCountChanged",
        "RefreshUI",
        "RefreshMenuPage",
    ],
};

static HOOK: ViewportHook<G2Binder> = ViewportHook::new(CONFIG);
static CAPACITY: AtomicI32 = AtomicI32::new(0);

/// One-time-resolved UFunction handles for the binder. Resolved at
/// install time, never moved. Pointer-stable per game session.
struct G2Binder {
    inv_get_inventory_items: usize,
    inv_initialize_item_slot: usize,
    bpf_cdo: usize,
    bpf_get_item_in_item_list_slot: usize,
    kismet_cdo: usize,
    kismet_pointer_event_get_wheel_delta: usize,
}

unsafe impl Send for G2Binder {}
unsafe impl Sync for G2Binder {}

impl ViewportBinder for G2Binder {
    /// Cache the items list once per scroll cycle so we don't
    /// re-call `GetInventoryItems` 40 times per rebind.
    /// Stays here because the inventory-list function and parameter layout belong to Grounded 2;
    /// Ueforge owns the reusable viewport rebind cycle.
    type RebindContext = GetInventoryItemsParms;

    /// Reads the items currently exposed by Grounded 2's inventory widget for one rebind cycle.
    /// Stays here because it invokes Grounded 2's inventory function with its exact parameter layout.
    unsafe fn begin_rebind(&self, widget: &UObject) -> GetInventoryItemsParms {
        let mut items = GetInventoryItemsParms {
            items: ue::TArray::default(),
        };
        let func = unsafe { &*(self.inv_get_inventory_items as *const UFunction) };
        unsafe {
            widget.process_event(func, &mut items as *mut _ as *mut c_void);
        }
        items
    }

    /// Converts Grounded 2's mouse-wheel event into the scroll amount used by the shared viewport.
    /// Stays here because the event layout and Kismet call are specific to this game's widget.
    unsafe fn mouse_wheel_delta(&self, parms: *const c_void) -> Option<f32> {
        if parms.is_null() {
            return None;
        }
        let view = unsafe { &*(parms as *const OnMouseWheelInputView) };
        if self.kismet_cdo == 0 || self.kismet_pointer_event_get_wheel_delta == 0 {
            return None;
        }
        let func = unsafe {
            &*(self.kismet_pointer_event_get_wheel_delta as *const UFunction)
        };
        // PointerEvent_GetWheelDelta is a Final|Native BlueprintCallable
        // helper. Setting FUNC_NATIVE is the path the engine takes
        // when invoking it from BP, and matches what worked pre-extraction.
        let saved_flags = func.function_flags();
        func.set_function_flags(saved_flags | ue::offsets::FUNC_NATIVE);
        let mut p = PointerEventGetWheelDeltaParms {
            input: view.mouse_event,
            return_value: 0.0,
            _pad: 0,
        };
        unsafe {
            (&*(self.kismet_cdo as *const UObject))
                .process_event(func, &mut p as *mut _ as *mut c_void);
        }
        func.set_function_flags(saved_flags);
        Some(p.return_value)
    }

    /// Binds one visible inventory slot to the item at its absolute backpack position.
    /// Stays here because Grounded 2 defines both Blueprint calls and their parameter layouts;
    /// Ueforge owns the page and slot iteration.
    unsafe fn bind_slot(
        &self,
        widget: &UObject,
        ctx: &GetInventoryItemsParms,
        slot: &UObject,
        absolute_index: i32,
    ) {
        // Resolve the item at the absolute index from the cached
        // items list using the BPF helper (it knows how to handle
        // out-of-bounds gracefully. Returns null Item).
        let bpf_func = unsafe {
            &*(self.bpf_get_item_in_item_list_slot as *const UFunction)
        };
        let mut bpf_parms = GetItemInItemListSlotParms {
            item_list: ue::TArray {
                data: ctx.items.data,
                num: ctx.items.num,
                max: ctx.items.max,
            },
            index: absolute_index,
            _pad: 0,
            world_context: widget as *const UObject as *mut UObject,
            item: std::ptr::null_mut(),
        };
        unsafe {
            (&*(self.bpf_cdo as *const UObject))
                .process_event(bpf_func, &mut bpf_parms as *mut _ as *mut c_void);
        }

        // Bind the visible slot to the resolved item.
        let init_func = unsafe {
            &*(self.inv_initialize_item_slot as *const UFunction)
        };
        let mut init_parms = InitializeItemSlotParms {
            item_slot: slot as *const UObject as *mut UObject,
            item: bpf_parms.item,
        };
        unsafe {
            widget.process_event(
                init_func,
                &mut init_parms as *mut _ as *mut c_void,
            );
        }
    }

    /// Reports the backpack size currently configured for Grounded 2.
    /// Stays here because the value combines this mod's setting and Backpack skill bonus.
    fn capacity(&self) -> i32 {
        CAPACITY.load(Ordering::Acquire)
    }
}

/// Installs scrolling and rebinding on Grounded 2's inventory widget.
/// Stays here because the widget classes and Blueprint functions belong to Grounded 2;
/// Ueforge owns the reusable viewport hook.
pub fn install(slot_count: i32) -> Result<ProcessEventHook, &'static str> {
    let inv_class = ClassRef::new("WBP_InventoryInterface_C")
        .get()
        .ok_or("WBP_InventoryInterface_C not loaded")?;
    let bpf_class = ClassRef::new("BPF_InventoryFunctions_C")
        .get()
        .ok_or("BPF_InventoryFunctions_C not loaded")?;
    let kismet_class = ClassRef::new("KismetInputLibrary")
        .get()
        .ok_or("KismetInputLibrary not loaded")?;

    let inv_get_inventory_items = inv_class
        .get_function("WBP_InventoryInterface_C", "GetInventoryItems")
        .ok_or("GetInventoryItems not found")? as *const UFunction
        as usize;
    let inv_initialize_item_slot = inv_class
        .get_function("WBP_InventoryInterface_C", "InitializeItemSlot")
        .ok_or("InitializeItemSlot not found")? as *const UFunction
        as usize;
    let bpf_cdo = bpf_class
        .class_default_object()
        .ok_or("BPF_InventoryFunctions_C CDO missing")?
        as *const UObject as usize;
    let bpf_get_item_in_item_list_slot = bpf_class
        .get_function("BPF_InventoryFunctions_C", "GetItemInItemListSlot")
        .ok_or("GetItemInItemListSlot not found")?
        as *const UFunction as usize;
    let kismet_cdo = kismet_class
        .class_default_object()
        .ok_or("KismetInputLibrary CDO missing")?
        as *const UObject as usize;
    let kismet_pointer_event_get_wheel_delta = kismet_class
        .get_function("KismetInputLibrary", "PointerEvent_GetWheelDelta")
        .ok_or("PointerEvent_GetWheelDelta not found")?
        as *const UFunction as usize;

    CAPACITY.store(slot_count, Ordering::Release);

    HOOK.install(G2Binder {
        inv_get_inventory_items,
        inv_initialize_item_slot,
        bpf_cdo,
        bpf_get_item_in_item_list_slot,
        kismet_cdo,
        kismet_pointer_event_get_wheel_delta,
    })
}

/// Updates the number of backpack positions exposed by the inventory window.
/// Stays here because Grounded 2's inventory patch and Backpack skill choose this value.
pub fn update_slot_count(slot_count: i32) {
    CAPACITY.store(slot_count, Ordering::Release);
    ueforge::log!("inv hook: slot_count updated to {}", slot_count);
}

/// Reports the backpack size currently presented by the inventory hook.
/// Stays here because it exposes Grounded 2 mod state to this mod's debug snapshot.
pub fn current_slot_count() -> i32 {
    CAPACITY.load(Ordering::Acquire)
}

const _: () = {
    let _ = F_POINTER_EVENT_SIZE;
};
