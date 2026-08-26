// VTable patching primitives + RAII ProcessEvent hook + small helpers
// that show up in every PE-trampoline-shaped feature.

#![allow(dead_code)]

pub mod engine_tick;
pub mod function_table;
pub mod install;
pub mod lazy_function;
pub mod process_event;
pub mod registry;
pub mod vtable;

pub use install::{RetryPolicy, install_immediate_or_log, install_with_backoff};
pub use lazy_function::LazyFunctionPtr;
pub use process_event::{
    HookDef, OriginalProcessEvent, ProcessEventHook, installed_defs, leaked_entry_count,
    panic_count_total,
};
pub use registry::{HOOK_REGISTRY, HookRegistry, register, register_many, shutdown_all};

use crate::ue::{UFunction, find_class_fast};

/// Resolve a UFunction by `(class_name, function_name)` and return
/// its address as a `usize`. Stash the result in an `AtomicUsize`
/// at hook-install time, then match by pointer identity in the
/// trampoline (`incoming_ptr == cached`) instead of calling
/// `function.name()`. Which would resolve an FName on every PE
/// call and dwarf the cost of the actual hook work.
///
/// Returns `None` if the class isn't loaded yet or doesn't expose
/// a function by that name. Use `function_ptr_required` to bail
/// the whole install path on missing functions; use this for
/// optional ones (e.g. UFunctions that exist on later patches).
pub fn function_ptr(class_name: &str, function_name: &str) -> Option<usize> {
    let class = find_class_fast(class_name)?;
    let func = class.get_function(class_name, function_name)?;
    Some(func as *const UFunction as usize)
}

/// `Result`-returning variant of [`function_ptr`]. Embeds the
/// missing-function diagnostic in the error so install-path
/// callers can propagate it directly.
pub fn function_ptr_required(
    class_name: &'static str,
    function_name: &'static str,
) -> Result<usize, &'static str> {
    function_ptr(class_name, function_name).ok_or(function_name)
}
