// Minimal Unreal Engine SDK shim. Mirrors enough of the Dumper-7 surface
// for our hooks and patches: UObject + UClass + UFunction + FName + FString
// + TArray + GObjects walker + find_class_fast.
//
// All `unsafe` lives in this module. Higher-level code (patch.rs, inv_hook,
// etc.) uses the safe wrappers.

#![allow(dead_code)] // skeleton state; consumed by upcoming hook + patch modules

pub mod actor;
pub mod class_ref;
pub mod damage_info;
pub mod class_tweak;
pub mod core_types;
pub mod datatable;
pub mod field;
pub mod fname;
pub mod fstring;
pub mod offsets;
pub mod pe_call;
pub mod platform;
pub mod player;
pub mod probe;
pub mod resolvers;
pub mod status_effect;
pub mod struct_fields;
pub mod tarray;
pub mod tmap;
pub mod typed_field;
pub mod uobject;

pub use class_ref::ClassRef;
pub use core_types::{EStatusEffectValueType, FDataTableRowHandle, FGuid, FWeakObjectPtr};
pub use player::PlayerRef;
pub use typed_field::TypedField;
pub use fname::FName;
pub use fstring::FString;
pub use offsets::{GObjectsLayout, Platform, PlatformOffsets};
pub use tarray::TArray;
pub use uobject::{
    GObjectsView, ProcessEventFn, Runtime, UClass, UFunction, UObject, find_class_fast,
    init_runtime, runtime, try_runtime,
};

/// Read a `Copy` value from a raw pointer at a byte offset.
///
/// # Safety
/// `ptr.add(offset)` must be valid for `size_of::<T>()` bytes.
pub unsafe fn read_at<T: Copy>(ptr: *const u8, offset: usize) -> T {
    unsafe { (ptr.add(offset) as *const T).read_unaligned() }
}

/// Write a `Copy` value to a raw pointer at a byte offset.
///
/// # Safety
/// `ptr.add(offset)` must be valid for `size_of::<T>()` bytes.
pub unsafe fn write_at<T: Copy>(ptr: *const u8, offset: usize, value: T) {
    unsafe { (ptr.add(offset) as *mut T).write_unaligned(value) }
}

/// Look up a class by name and pass its first non-CDO instance to
/// `f`. Returns `None` if the class isn't loaded or no live
/// instance exists yet.
///
/// Convenience wrapper for ad-hoc snapshot / debug call sites that
/// don't deserve a `static ClassRef` (typically: read a singleton
/// data asset's fields once into a serializable view).
pub fn with_first_instance_of<R>(
    class_name: &str,
    f: impl FnOnce(&UObject) -> R,
) -> Option<R> {
    ClassRef::new_dynamic(class_name).with_first_instance(f)
}

/// Look up a class by name and pass its first matching CDO to
/// `f`. Walks every `is_a(class) && is_default_object`, so
/// subclass CDOs match the parent name too.
pub fn with_first_cdo_of<R>(
    class_name: &str,
    f: impl FnOnce(&UObject) -> R,
) -> Option<R> {
    ClassRef::new_dynamic(class_name).with_first_cdo(f)
}
