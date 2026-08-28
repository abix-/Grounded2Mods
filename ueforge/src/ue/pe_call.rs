//! `process_event` invocation helpers for game-thread UFunction
//! calls. Covers the universal pattern every UE5 mod runs from
//! its debug endpoints / queued ops / ImGui buttons:
//!
//! 1. Resolve a `UFunction*` on a class by name.
//! 2. Build a `#[repr(C)]` parm struct.
//! 3. Call `process_event(target, function, &mut parms)`.
//! 4. Optionally read OUT parm fields back.
//!
//! Health ops (`AddHealth`, `SetCurrentHealth`,
//! `ApplyDamageFromInfo`) and inventory ops
//! (`AddItem`, `RemoveItem`, `GetItemCount`) all fit. The parm
//! struct shape stays game-specific (each game's SDK has its
//! own `#[repr(C)]` layout); the resolve + invoke + error
//! plumbing is universal.

use std::ffi::c_void;

use crate::ue::{ClassRef, UFunction, UObject};

fn find_object_function<'a>(
    target: &'a UObject,
    class_name: &str,
    function_name: &str,
) -> Result<&'a UFunction, String> {
    let class = target.class().ok_or("instance has no class")?;
    class
        .get_function(class_name, function_name)
        .ok_or_else(|| format!("{class_name} has no {function_name}"))
}

unsafe fn invoke_bytes(
    target: &UObject,
    function: &UFunction,
    function_name: &str,
    parms: &mut [u8],
) -> Result<(), String> {
    let required = function.parms_size() as usize;
    if parms.len() < required {
        return Err(format!(
            "{function_name} needs {required} parm bytes, got {}",
            parms.len()
        ));
    }
    unsafe {
        target.process_event(function, parms.as_mut_ptr() as *mut c_void);
    }
    Ok(())
}

/// Resolve a UFunction from a live object's class and invoke it
/// with a caller-owned byte buffer. The buffer is checked against
/// the UFunction's `ParmsSize` before Unreal can read or write it.
/// OUT parameters remain in `parms` when the call returns.
///
/// # Safety
/// The bytes must match the UFunction's parameter layout and the
/// call must run on the game thread.
pub unsafe fn call_ufunction_bytes<'a>(
    target: &'a UObject,
    class_name: &str,
    function_name: &str,
    parms: &mut [u8],
) -> Result<&'a UFunction, String> {
    let function = find_object_function(target, class_name, function_name)?;
    unsafe { invoke_bytes(target, function, function_name, parms)? };
    Ok(function)
}

/// Resolve a UFunction, allocate a zeroed parameter block from
/// its own `ParmsSize`, and invoke it. This is for engine event
/// handlers whose declared input values should all use defaults.
///
/// # Safety
/// The call must run on the game thread, and zero must be a valid
/// default bit pattern for every declared parameter.
pub unsafe fn call_ufunction_zeroed<'a>(
    target: &'a UObject,
    class_name: &str,
    function_name: &str,
) -> Result<&'a UFunction, String> {
    let function = find_object_function(target, class_name, function_name)?;
    let mut parms = vec![0u8; function.parms_size().max(1) as usize];
    unsafe { invoke_bytes(target, function, function_name, &mut parms)? };
    Ok(function)
}

/// Call a UFunction by name on `target`, passing a `#[repr(C)]`
/// parm struct by mutable reference. The engine may write OUT
/// fields back into `parms` during the call. Caller reads them
/// after this returns.
///
/// Returns an error string if the function isn't found on
/// `class`. The parm struct is always passed; if the function
/// has no parms, pass `&mut ()`.
///
/// # Safety
/// `P` MUST be a `#[repr(C)]` layout that matches the
/// UFunction's parm shape exactly (per the game's SDK). Mismatch
/// is undefined behavior. Caller MUST be on the game thread
/// (`process_event` re-enters the engine's PE machinery).
///
/// ```ignore
/// use ueforge::ue::ClassRef;
/// use ueforge::ue::pe_call::call_ufunction;
///
/// static HEALTH: ClassRef = ClassRef::new("HealthComponent");
///
/// #[repr(C)]
/// struct AddHealthParms { amount: f32, causer: *mut std::ffi::c_void }
///
/// let mut parms = AddHealthParms { amount: 50.0, causer: std::ptr::null_mut() };
/// unsafe { call_ufunction(hc, &HEALTH, "AddHealth", &mut parms) }?;
/// ```
pub unsafe fn call_ufunction<P>(
    target: &UObject,
    class: &ClassRef,
    function_name: &str,
    parms: &mut P,
) -> Result<(), String> {
    let func = class
        .find_function(function_name)
        .ok_or_else(|| format!("{}::{} UFunction not found", class.name(), function_name))?;
    unsafe {
        target.process_event(func, parms as *mut P as *mut c_void);
    }
    Ok(())
}

/// Same as [`call_ufunction`] but returns a JSON object with
/// `"function"` + `"class"` + the caller-supplied `extra` map
/// folded in. Convenience for debug-endpoint executors that
/// always return JSON.
pub unsafe fn call_ufunction_json<P>(
    target: &UObject,
    class: &ClassRef,
    function_name: &str,
    parms: &mut P,
    extra: serde_json::Value,
) -> Result<serde_json::Value, String> {
    unsafe { call_ufunction(target, class, function_name, parms)? };
    let mut out = serde_json::json!({
        "class": class.name(),
        "function": function_name,
    });
    if let (Some(o), Some(e)) = (out.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
    }
    Ok(out)
}
