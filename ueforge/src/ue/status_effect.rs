//! UE5 status-effect surface: data-table row mutation +
//! `CreateAndAddEffect` UFunction invocation.
//!
//! UE5 RPGs route gear / perk / status effects through one of
//! two patterns. The canonical one (used by Maine, and by every
//! game that copies it) is **row-driven**:
//!
//! 1. A `UDataTable` (e.g. `Table_StatusEffects`) holds rows of
//!    a `UScriptStruct` whose layout includes a `Type` (enum) +
//!    `Value` (f32) field.
//! 2. The game adds an effect to a target by calling a UFunction
//!    on the target's `UStatusEffectComponent` (or the game's
//!    analogue), passing an `FDataTableRowHandle` `(table*, row_fname)`.
//! 3. The component reads the row's Value and applies the
//!    effect.
//!
//! We can change a stat dynamically by mutating the row's Value
//! field BEFORE calling `CreateAndAddEffect`. Then every consumer
//! of that row. Our skill, AND any vanilla actor that uses the
//! same row. Sees the new value. Pick a benign / unused row
//! per skill to avoid cross-contamination.
//!
//! ```ignore
//! use ueforge::ue::{ClassRef, status_effect, TypedField};
//!
//! static SE_COMPONENT: ClassRef = ClassRef::new("StatusEffectComponent");
//!
//! // Update the row...
//! unsafe {
//!     status_effect::write_row_value(row_ptr, status_effect::DEFAULT_VALUE_OFFSET, 0.5);
//! }
//!
//! // ...then add the effect to the player's component.
//! let func = SE_COMPONENT.find_function("CreateAndAddEffect").unwrap();
//! status_effect::create_and_add_effect(
//!     player_sec, func, table, row_fname,
//! );
//! ```

use std::ffi::c_void;

use crate::ue::{FName, UFunction, UObject};

/// Game-supplied offsets for reading the active status effects on an actor.
#[derive(Debug, Clone, Copy)]
pub struct StatusEffectLayout {
    pub component_offset: usize,
    pub effects_array_offset: usize,
    pub row_handle_offset: usize,
    pub type_offset: usize,
    pub value_offset: usize,
    pub max_effects: usize,
}

/// One active status effect resolved through its data-table row handle.
#[derive(Debug, Clone)]
pub struct StatusEffectEntry {
    pub row: String,
    pub table: String,
    pub stat_type: Option<u8>,
    pub value: Option<f32>,
}

/// Default byte offset of the `Value: f32` field within a UE5
/// status-effect row struct (`FStatusEffectData` in Maine; varies
/// per game). Override with the game's actual struct offset if it
/// differs.
pub const DEFAULT_VALUE_OFFSET: usize = 0x34;

/// Default byte offset of the `Type: u8` field (the
/// `EStatusEffectType` enum) within a UE5 status-effect row
/// struct.
pub const DEFAULT_TYPE_OFFSET: usize = 0x30;

/// Read the f32 value field at `offset` from a status-effect
/// row's raw bytes.
///
/// # Safety
/// `row_ptr` must be a live pointer into a `UDataTable` row
/// whose backing struct has an `f32` at `offset`.
pub unsafe fn read_row_value(row_ptr: *const u8, offset: usize) -> f32 {
    unsafe { (row_ptr.add(offset) as *const f32).read_unaligned() }
}

/// Read the u8 stat-type field at `offset` from a status-effect
/// row's raw bytes.
///
/// # Safety
/// Same as [`read_row_value`].
pub unsafe fn read_row_type(row_ptr: *const u8, offset: usize) -> u8 {
    unsafe { (row_ptr.add(offset) as *const u8).read_unaligned() }
}

/// Mutate a status-effect row's f32 value in-place. Returns the
/// previous value.
///
/// # Safety
/// Same as [`read_row_value`]. Mutating a shared row affects
/// every consumer of that row. Pick a benign / per-skill row
/// to avoid cross-contamination.
pub unsafe fn write_row_value(row_ptr: *const u8, offset: usize, new_value: f32) -> f32 {
    unsafe {
        let p = row_ptr.add(offset) as *mut f32;
        let prev = p.read_unaligned();
        p.write_unaligned(new_value);
        prev
    }
}

/// Read the active status effects attached to `actor` using the supplied
/// game layout.
pub fn read_active(actor: &UObject, layout: StatusEffectLayout) -> Vec<StatusEffectEntry> {
    let mut entries = Vec::new();
    // SAFETY: the caller supplies the actor's component-pointer offset.
    let component = unsafe {
        let ptr: *mut UObject = actor
            .field_ptr(layout.component_offset)
            .cast::<*mut UObject>()
            .read_unaligned();
        match ptr.as_ref() {
            Some(component) => component,
            None => return entries,
        }
    };
    // SAFETY: the caller supplies the component's TArray header offset.
    let (data_ptr, count) = unsafe {
        let base = component.field_ptr(layout.effects_array_offset);
        let data = (base as *const *const *mut UObject).read_unaligned();
        let count = (base.add(8) as *const i32).read_unaligned();
        (data, count.max(0) as usize)
    };
    if data_ptr.is_null() || count == 0 {
        return entries;
    }

    for index in 0..count.min(layout.max_effects) {
        // SAFETY: the TArray header supplies `count` entries, capped by the
        // caller, and each entry is a UObject pointer.
        let effect = unsafe {
            let ptr = data_ptr.add(index).read_unaligned();
            match ptr.as_ref() {
                Some(effect) => effect,
                None => continue,
            }
        };
        // SAFETY: the caller supplies the FDataTableRowHandle offset, whose
        // first field is the table pointer and whose second field is FName.
        let (table_ptr, raw_fname) = unsafe {
            let handle = effect.field_ptr(layout.row_handle_offset);
            (
                handle.cast::<*mut UObject>().read_unaligned(),
                (handle.add(8) as *const u64).read_unaligned(),
            )
        };
        // SAFETY: the active UE runtime owns the FName resolver.
        let row = unsafe {
            crate::ue::runtime()
                .name_resolver
                .to_string(FName::from_u64(raw_fname))
        };
        // SAFETY: table_ptr came from the live effect's row handle.
        let table = unsafe {
            table_ptr
                .as_ref()
                .map(UObject::full_name)
                .unwrap_or_else(|| "<null-table>".to_string())
        };
        // SAFETY: table_ptr and raw_fname came from the live row handle;
        // the caller supplies the row's Type and Value offsets.
        let row_meta = unsafe {
            table_ptr.as_ref().and_then(|table| {
                crate::ue::tmap::find_value_by_fname_key(
                    table,
                    crate::ue::offsets::datatable::ROW_MAP,
                    raw_fname,
                )
                .map(|row_ptr| {
                    (
                        read_row_type(row_ptr, layout.type_offset),
                        read_row_value(row_ptr, layout.value_offset),
                    )
                })
            })
        };
        let (stat_type, value) = match row_meta {
            Some((stat_type, value)) => (Some(stat_type), Some(value)),
            None => (None, None),
        };
        entries.push(StatusEffectEntry {
            row,
            table,
            stat_type,
            value,
        });
    }
    entries
}

/// `FDataTableRowHandle` parm shape for `CreateAndAddEffect`.
///
/// Cooked UE5 layout:
///
/// ```text
/// FDataTableRowHandle StatusEffectRowHandle  // 16 bytes
///   - UDataTable* DataTable                  // 8 bytes
///   - FName       RowName                    // 8 bytes (u64)
/// class UStatusEffect* ReturnValue           // 8 bytes (OUT)
/// ```
///
/// Total: 24 bytes. Used as the parm buffer for the UFunction
/// call.
#[repr(C)]
struct CreateAndAddEffectParms {
    data_table: *const UObject,
    row_fname: u64,
    return_value: *mut UObject,
}

/// Invoke a `CreateAndAddEffect`-shaped UFunction on `component`
/// with the given table + row FName. Returns the
/// `UStatusEffect*` the engine wrote to the OUT parm (or null
/// if the engine refused).
///
/// `function_ptr` is the resolved `&UFunction`. Typically
/// cached at install via `ClassRef::find_function("CreateAndAddEffect")`.
///
/// MUST be called on the game thread (process_event re-enters
/// the engine's PE machinery). Enqueue via `Queue` if calling
/// from off-thread.
pub fn create_and_add_effect(
    component: &UObject,
    function_ptr: &UFunction,
    table: &UObject,
    row_fname: u64,
) -> *mut UObject {
    let mut parms = CreateAndAddEffectParms {
        data_table: table as *const UObject,
        row_fname,
        return_value: std::ptr::null_mut(),
    };
    unsafe {
        component.process_event(function_ptr, &mut parms as *mut _ as *mut c_void);
    }
    parms.return_value
}

/// `RemoveEffect`-style invocation. Many games expose a
/// `RemoveStatusEffect(FDataTableRowHandle)` UFunction that
/// undoes a previous add. Same parm shape.
pub fn remove_effect(
    component: &UObject,
    function_ptr: &UFunction,
    table: &UObject,
    row_fname: u64,
) {
    let mut parms = CreateAndAddEffectParms {
        data_table: table as *const UObject,
        row_fname,
        return_value: std::ptr::null_mut(),
    };
    unsafe {
        component.process_event(function_ptr, &mut parms as *mut _ as *mut c_void);
    }
}
