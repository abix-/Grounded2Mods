//! Viewport-paging hook framework. Hooks a UE5 inventory widget's
//! ProcessEvent slot and provides:
//!
//! - **Mouse-wheel scroll** through pages of a fixed-size visible
//!   grid, with mouse-wheel delta detection via
//!   `KismetInputLibrary.PointerEvent_GetWheelDelta`.
//! - **Per-widget viewport-start state** so multiple inventory
//!   widget instances each remember their own scroll position.
//! - **Synthetic-refresh re-entrance guard** so post-rebind refresh
//!   events don't recurse into the hook.
//! - **Post-refresh rebind** so when the game itself fires an
//!   inventory-refresh event (item picked up, count changed, etc),
//!   the framework restores the current viewport position.
//! - **Construct reset** to viewport-start = 0 on widget construct.
//! - **Auto-bypass** when capacity <= page_size (no need to scroll).
//!
//! Game crates supply:
//! - A [`ViewportConfig`] of class + UFunction names + offsets.
//! - A [`ViewportBinder`] impl that knows how to (a) extract the
//!   mouse-wheel delta from the game's `OnMouseWheel` parm bytes
//!   and (b) bind one visible slot widget to display the item at
//!   a given absolute slot index in the underlying inventory.
//!
//! Both can typically be `const`-friendly thin wrappers; the
//! complex viewport-state + hook plumbing all lives here.
//!
//! ```ignore
//! use ueforge::inventory::viewport::{
//!     ViewportConfig, ViewportBinder, ViewportHook,
//! };
//!
//! const CONFIG: ViewportConfig = ViewportConfig {
//!     widget_class: "WBP_InventoryInterface_C",
//!     item_grid_offset: 0x0430,
//!     page_size: 40,        // 4 rows * 10 cols visible
//!     scroll_step: 10,      // one row per wheel tick
//!     construct_fn: "Construct",
//!     on_mouse_wheel_fn: "OnMouseWheel",
//!     refresh_fns: &["PopulateItemGrid", "RefreshUI", "OnInventoryChanged"],
//! };
//!
//! struct Binder { /* game-specific UFunction handles + parm shapes */ }
//!
//! impl ViewportBinder for Binder {
//!     unsafe fn mouse_wheel_delta(&self, parms: *const c_void) -> Option<f32> { ... }
//!     unsafe fn bind_slot(&self, widget: &UObject, slot: &UObject, abs_index: i32) { ... }
//!     fn capacity(&self) -> i32 { CAPACITY.load(Ordering::Acquire) }
//! }
//!
//! static HOOK: ViewportHook<Binder> = ViewportHook::new(CONFIG);
//!
//! // From `on_unreal_init`:
//! HOOK.install(Binder::new()?)?;
//!
//! // From settings change / RPG apply:
//! HOOK.set_capacity(new_capacity);
//! ```

use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;

use crate::hook::{OriginalProcessEvent, ProcessEventHook};
use crate::ue::{ClassRef, UFunction, UObject};

/// Per-game configuration for a viewport-paging hook. All
/// `&'static str` so callers declare it as a `const`.
#[derive(Debug, Clone, Copy)]
pub struct ViewportConfig {
    /// Widget class to hook ProcessEvent on.
    pub widget_class: &'static str,
    /// Byte offset of the panel widget that holds the visible item
    /// slots, on the widget instance.
    pub item_grid_offset: usize,
    /// Visible slot count = page size. Typical: rows * columns
    /// (e.g. 4 * 10 = 40 for a 4x10 grid).
    pub page_size: i32,
    /// Slots scrolled per mouse-wheel tick. Typical: one row
    /// (e.g. 10 for a 10-column grid).
    pub scroll_step: i32,
    /// "Construct" UFunction name. Viewport resets to 0 on
    /// widget construction.
    pub construct_fn: &'static str,
    /// "OnMouseWheel" UFunction name.
    pub on_mouse_wheel_fn: &'static str,
    /// Refresh-trigger UFunction names. The hook re-binds the
    /// visible slots after the original call returns from any of
    /// these (so the game's own refresh path doesn't reset the
    /// viewport to slot 0). Empty slice if not needed.
    pub refresh_fns: &'static [&'static str],
}

/// Game-specific knowledge the viewport hook needs from the
/// game crate.
pub trait ViewportBinder: Send + Sync + 'static {
    /// Per-rebind context the binder caches once and reuses for
    /// every visible slot in one scroll cycle. Typical: cache the
    /// inventory items list so we don't re-call `GetInventoryItems`
    /// `page_size` times. Use `()` if no cache is needed.
    type RebindContext;

    /// Prepare a rebind cycle. Called once per scroll, before
    /// any `bind_slot` calls. The returned context is passed to
    /// each `bind_slot` and dropped at the end of the cycle.
    ///
    /// # Safety
    /// Caller is on the game thread inside the PE trampoline.
    unsafe fn begin_rebind(&self, widget: &UObject) -> Self::RebindContext;

    /// Read the mouse-wheel scroll delta from the game's
    /// `OnMouseWheel` UFunction parm bytes. Return None to
    /// suppress scrolling for this fire (e.g. a modifier-key
    /// gate or invalid parms).
    ///
    /// Typical impl casts `parms` to the game's `OnMouseWheel`
    /// parm struct, reads the embedded `FPointerEvent`, and calls
    /// `KismetInputLibrary.PointerEvent_GetWheelDelta`.
    ///
    /// # Safety
    /// `parms` is the pointer the engine handed to ProcessEvent.
    /// The impl MUST validate the layout matches the game's
    /// actual `OnMouseWheel` UFunction parm shape.
    unsafe fn mouse_wheel_delta(&self, parms: *const c_void) -> Option<f32>;

    /// Bind one visible slot widget to display the item at
    /// `absolute_index` in the underlying inventory. Typically
    /// invokes the game's "initialize item slot" UFunction with
    /// the slot widget + a resolved item handle from `ctx`.
    ///
    /// # Safety
    /// Caller MUST be on the game thread (the hook ensures
    /// this).
    unsafe fn bind_slot(
        &self,
        widget: &UObject,
        ctx: &Self::RebindContext,
        slot: &UObject,
        absolute_index: i32,
    );

    /// Current underlying inventory capacity. The hook reads
    /// this every scroll, so changes (e.g. an RPG `+slots`
    /// skill applying) take effect without re-installing.
    fn capacity(&self) -> i32;
}

/// Static viewport-paging hook. Construct as a `static` with
/// [`ViewportHook::new`]; install once at module init with
/// [`install`](Self::install).
pub struct ViewportHook<B: ViewportBinder> {
    config: ViewportConfig,
    state: OnceLock<HookState<B>>,
}

struct HookState<B: ViewportBinder> {
    binder: B,
    fn_construct: usize,
    fn_on_mouse_wheel: usize,
    fn_refresh: Vec<usize>,
    panel_get_children_count: usize,
    panel_get_child_at: usize,
    viewport_starts: Mutex<Vec<(usize, i32)>>,
    in_synthetic_refresh: AtomicBool,
}

// SAFETY: HookState carries B (ViewportBinder is Send + Sync
// by trait bound), AtomicBool, cached usize-encoded UFunction
// pointers (immutable after install), and Mutex-guarded
// per-widget state.
unsafe impl<B: ViewportBinder> Send for HookState<B> {}
// SAFETY: see Send impl. Cached pointers are read-only after
// install; mutex-guarded state is the canonical concurrent-
// access pattern.
unsafe impl<B: ViewportBinder> Sync for HookState<B> {}

impl<B: ViewportBinder> ViewportHook<B> {
    pub const fn new(config: ViewportConfig) -> Self {
        Self {
            config,
            state: OnceLock::new(),
        }
    }

    /// Install the hook. Resolves the configured UFunctions on
    /// the widget class + on UMG `PanelWidget`, stashes the
    /// binder, and installs the ProcessEvent trampoline. The
    /// returned [`ProcessEventHook`] should be leaked
    /// (`std::mem::forget`) so it survives worker-thread exit.
    ///
    /// `ueforge::hook::install_immediate_or_log` works:
    ///
    /// ```ignore
    /// ueforge::hook::install_immediate_or_log(
    ///     "inv hook",
    ///     || HOOK.install(my_binder),
    ///     |h| h.class_name(),
    /// );
    /// ```
    pub fn install(&'static self, binder: B) -> Result<ProcessEventHook, &'static str> {
        let widget_class = ClassRef::new_dynamic(self.config.widget_class)
            .get()
            .ok_or("ViewportHook: widget class not loaded")?;
        let panel_class = ClassRef::new_dynamic("PanelWidget")
            .get()
            .ok_or("ViewportHook: PanelWidget class not loaded")?;

        let fn_construct = lookup_fn(widget_class, self.config.construct_fn)?;
        let fn_on_mouse_wheel = lookup_fn(widget_class, self.config.on_mouse_wheel_fn)?;
        let mut fn_refresh = Vec::with_capacity(self.config.refresh_fns.len());
        for name in self.config.refresh_fns {
            // Refresh fns are optional (best-effort post-refresh
            // rebind); skip any that don't exist on the class.
            if let Some(f) = widget_class.get_function(self.config.widget_class, name) {
                fn_refresh.push(f as *const UFunction as usize);
            }
        }
        let panel_get_children_count = lookup_fn(panel_class, "GetChildrenCount")?;
        let panel_get_child_at = lookup_fn(panel_class, "GetChildAt")?;

        if self
            .state
            .set(HookState {
                binder,
                fn_construct,
                fn_on_mouse_wheel,
                fn_refresh,
                panel_get_children_count,
                panel_get_child_at,
                viewport_starts: Mutex::new(Vec::new()),
                in_synthetic_refresh: AtomicBool::new(false),
            })
            .is_err()
        {
            return Err("ViewportHook: install called twice");
        }

        crate::log!(
            "inventory/viewport: hooking {} (page_size={}, scroll_step={})",
            self.config.widget_class,
            self.config.page_size,
            self.config.scroll_step
        );
        // Stash a pointer to `self` in a side channel so the
        // ProcessEventHook callback (which can't carry user data)
        // can look us up. Single-static-per-class is enforced by
        // the OnceLock above.
        register_hook_callback::<B>(self);
        ProcessEventHook::install(self.config.widget_class, dispatch::<B>)
    }
}

fn lookup_fn(class: &crate::ue::UClass, name: &str) -> Result<usize, &'static str> {
    class
        .get_function(class.as_object().name().as_str(), name)
        .map(|f| f as *const UFunction as usize)
        .ok_or("ViewportHook: required UFunction not found")
}

// ----------------------------------------------------------------
// Single-static dispatch table. ProcessEventHook callbacks are
// `fn(&UObject, &UFunction, *mut c_void, OriginalProcessEvent)`.
// no user-data slot. We store a *const () pointer to the
// `ViewportHook<B>` static keyed by class name; the callback
// looks it up.
// ----------------------------------------------------------------

// Hook-pointer side channel. The ProcessEvent callback signature
// has no user-data slot, so we stash a pointer (as usize, for
// Sync) keyed by class name. One static `ViewportHook<B>` per
// class is the supported pattern.
static DISPATCH_TABLE: Mutex<Vec<(&'static str, usize)>> = Mutex::new(Vec::new());

fn register_hook_callback<B: ViewportBinder>(hook: &'static ViewportHook<B>) {
    let mut t = DISPATCH_TABLE.lock();
    let class = hook.config.widget_class;
    let addr = hook as *const ViewportHook<B> as usize;
    if let Some(slot) = t.iter_mut().find(|(c, _)| *c == class) {
        slot.1 = addr;
    } else {
        t.push((class, addr));
    }
}

fn dispatch<B: ViewportBinder>(
    this: &UObject,
    function: &UFunction,
    parms: *mut c_void,
    original: OriginalProcessEvent,
) {
    let Some(class_name) = this.class().map(|c| c.as_object().name()) else {
        // SAFETY: original was captured from the engine vtable
        // at install time; this/function/parms are the engine-
        // supplied call args. Forwarded unchanged on early-out.
        unsafe { original.call(this, function, parms) };
        return;
    };
    let addr = {
        let table = DISPATCH_TABLE.lock();
        table
            .iter()
            .find(|(c, _)| class_name.contains(c))
            .map(|(_, addr)| *addr)
    };
    let Some(addr) = addr else {
        // SAFETY: same trampoline contract as above; no hook
        // matches this class, forward to the engine.
        unsafe { original.call(this, function, parms) };
        return;
    };
    // SAFETY: `addr` came from `register_dispatch` which stored
    // `(class, &'static ViewportHook<B> as usize)`; the static
    // lifetime + same B parameter is the install-side invariant.
    let hook: &ViewportHook<B> = unsafe { &*(addr as *const ViewportHook<B>) };
    on_event(hook, this, function, parms, original);
}

fn on_event<B: ViewportBinder>(
    hook: &ViewportHook<B>,
    this: &UObject,
    function: &UFunction,
    parms: *mut c_void,
    original: OriginalProcessEvent,
) {
    let Some(state) = hook.state.get() else {
        // SAFETY: trampoline contract; hook state not ready yet,
        // forward unchanged.
        unsafe { original.call(this, function, parms) };
        return;
    };
    if state.in_synthetic_refresh.load(Ordering::Acquire) {
        // SAFETY: same; re-entrance guard short-circuits.
        unsafe { original.call(this, function, parms) };
        return;
    }

    let fn_id = function as *const UFunction as usize;
    let cfg = &hook.config;

    if fn_id == state.fn_construct {
        set_viewport_start(state, this, 0);
    }

    // SAFETY: forward to the captured engine ProcessEvent. We do
    // not mutate parms before the call; pre-refresh state bookkeeping
    // happens above. Post-call handlers run below.
    unsafe { original.call(this, function, parms) };

    if fn_id == state.fn_on_mouse_wheel {
        handle_mouse_wheel(hook, state, this, parms);
    } else if state.fn_refresh.iter().any(|&f| f == fn_id) && get_viewport_start(state, this) != 0 {
        let cur = get_viewport_start(state, this);
        let _ = rebind(hook, state, this, cur, "post-refresh");
    }
    let _ = cfg;
}

fn handle_mouse_wheel<B: ViewportBinder>(
    hook: &ViewportHook<B>,
    state: &HookState<B>,
    widget: &UObject,
    parms: *mut c_void,
) {
    let capacity = state.binder.capacity();
    if capacity <= hook.config.page_size || parms.is_null() {
        return;
    }
    // SAFETY: forwarding the engine's parm block to the binder's
    // typed decoder; the binder's `unsafe fn mouse_wheel_delta`
    // contract is to read the configured offsets within `parms`
    // without retaining the pointer.
    let Some(delta) = (unsafe { state.binder.mouse_wheel_delta(parms) }) else {
        return;
    };
    let cur = get_viewport_start(state, widget);
    let next = if delta > 0.001 {
        clamp_start(
            cur - hook.config.scroll_step,
            capacity,
            hook.config.page_size,
        )
    } else if delta < -0.001 {
        clamp_start(
            cur + hook.config.scroll_step,
            capacity,
            hook.config.page_size,
        )
    } else {
        cur
    };
    if next != cur {
        let _ = rebind(hook, state, widget, next, "mouse-wheel");
    }
}

fn rebind<B: ViewportBinder>(
    hook: &ViewportHook<B>,
    state: &HookState<B>,
    widget: &UObject,
    new_start: i32,
    reason: &str,
) -> Result<(), &'static str> {
    // SAFETY: `widget` is the engine-supplied panel UObject;
    // hook.config.item_grid_offset is the configured offset of the
    // *mut UObject pointer to the child item-grid panel within
    // that widget's layout.
    let item_grid_ptr = unsafe {
        widget
            .field_ptr(hook.config.item_grid_offset)
            .cast::<*mut UObject>()
            .read_unaligned()
    };
    if item_grid_ptr.is_null() {
        return Err("item_grid is null");
    }
    // SAFETY: pointer is non-null per the check above; the item-
    // grid is a live UObject child of the widget for the duration
    // of this rebind (game thread, no GC interleave).
    let item_grid = unsafe { &*item_grid_ptr };

    let child_count = panel_children_count(state, item_grid);
    if child_count != hook.config.page_size {
        if cfg!(debug_assertions) {
            crate::log!(
                "inventory/viewport: skip rebind: expected {} visible children, found {}",
                hook.config.page_size,
                child_count
            );
        }
        return Err("unexpected child count");
    }

    let capacity = state.binder.capacity();
    let clamped = clamp_start(new_start, capacity, hook.config.page_size);
    let old_start = get_viewport_start(state, widget);

    state.in_synthetic_refresh.store(true, Ordering::Release);
    // SAFETY: binder's `unsafe fn begin_rebind` contract: takes
    // the engine widget ref, returns a per-binder context that
    // lives until dropped at the end of this rebind. We're on
    // the game thread inside the trampoline.
    let ctx = unsafe { state.binder.begin_rebind(widget) };
    for visible in 0..hook.config.page_size {
        let child = panel_child_at(state, item_grid, visible);
        let Some(child) = child else { continue };
        let absolute = clamped + visible;
        // SAFETY: bind_slot's `unsafe fn` contract: re-binds the
        // child UObject's stored slot index to `absolute`. The
        // widget + child + ctx are all live UObjects produced by
        // the engine for this frame.
        unsafe {
            state.binder.bind_slot(widget, &ctx, child, absolute);
        }
    }
    drop(ctx);
    state.in_synthetic_refresh.store(false, Ordering::Release);

    set_viewport_start(state, widget, clamped);
    if cfg!(debug_assertions) {
        crate::log!(
            "inventory/viewport: scroll {}: start={} -> {}",
            reason,
            old_start,
            clamped
        );
    }
    Ok(())
}

fn panel_children_count<B: ViewportBinder>(state: &HookState<B>, panel: &UObject) -> i32 {
    #[repr(C)]
    struct IntReturn {
        v: i32,
    }
    // SAFETY: `state.panel_get_children_count` was cached at
    // install time from the resolved UFunction; it lives for the
    // process lifetime in GObjects.
    let func = unsafe { &*(state.panel_get_children_count as *const UFunction) };
    let mut parms = IntReturn { v: -1 };
    // SAFETY: panel is a live UObject; `func` is its
    // GetChildrenCount; parms holds the i32 return slot the
    // function writes through.
    unsafe {
        panel.process_event(func, &mut parms as *mut _ as *mut c_void);
    }
    parms.v
}

fn panel_child_at<B: ViewportBinder>(
    state: &HookState<B>,
    panel: &UObject,
    index: i32,
) -> Option<&'static UObject> {
    #[repr(C)]
    struct GetChildAt {
        index: i32,
        _pad: i32,
        ret: *mut UObject,
    }
    // SAFETY: cached UFunction pointer from install time.
    let func = unsafe { &*(state.panel_get_child_at as *const UFunction) };
    let mut parms = GetChildAt {
        index,
        _pad: 0,
        ret: std::ptr::null_mut(),
    };
    // SAFETY: panel live UObject + matching GetChildAt UFunction
    // + parms layout matches the engine's parm block.
    unsafe {
        panel.process_event(func, &mut parms as *mut _ as *mut c_void);
    }
    if parms.ret.is_null() {
        None
    } else {
        // SAFETY: ret was written by the engine; child UObject is
        // live for the rest of this frame at minimum (game thread,
        // no GC interleave inside the trampoline). We extend
        // lifetime to 'static which is the standard contract for
        // engine-returned UObject refs.
        Some(unsafe { &*parms.ret })
    }
}

fn clamp_start(value: i32, capacity: i32, page_size: i32) -> i32 {
    let max_start = (capacity - page_size).max(0);
    value.clamp(0, max_start)
}

fn get_viewport_start<B: ViewportBinder>(state: &HookState<B>, widget: &UObject) -> i32 {
    let key = widget as *const UObject as usize;
    state
        .viewport_starts
        .lock()
        .iter()
        .find_map(|(k, v)| if *k == key { Some(*v) } else { None })
        .unwrap_or(0)
}

fn set_viewport_start<B: ViewportBinder>(state: &HookState<B>, widget: &UObject, value: i32) {
    let key = widget as *const UObject as usize;
    let mut g = state.viewport_starts.lock();
    if let Some(entry) = g.iter_mut().find(|(k, _)| *k == key) {
        entry.1 = value;
    } else {
        g.push((key, value));
    }
}
