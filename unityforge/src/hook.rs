//! HarmonyLib wrapper. Each `Hook` represents one prefix/postfix
//! patch on a managed method. The Rust callback is an `extern
//! "C" fn` pointer; the shim wraps it in a managed delegate so
//! HarmonyLib can target it.
//!
//! K8s slot: Def=HookDef, Registry=HookRegistry
//!           (HOOK_REGISTRY singleton),
//!           Instance=PatchHandle, Controller=trampoline + Drop

use std::ffi::{CString, c_void};

use parking_lot::Mutex;

use crate::bridge::{self, PatchHandle};

/// One installed Harmony patch.
pub struct Hook {
    handle: PatchHandle,
    pub class_name: String,
    pub method_name: String,
    pub when: HookWhen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookWhen {
    Prefix,
    Postfix,
}

impl Hook {
    pub fn handle(&self) -> PatchHandle {
        self.handle
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        if let Some(bridge) = bridge::get() {
            (bridge.harmony_unpatch)(self.handle);
        }
    }
}

/// Install a prefix patch. The trampoline must be an `extern "C"
/// fn(*const c_void) -> i32`. Returning non-zero from a prefix
/// tells HarmonyLib to skip the original method.
pub fn patch_prefix(
    class_name: &str,
    method_name: &str,
    prefix_fn: extern "C" fn(*const c_void) -> i32,
) -> Result<Hook, String> {
    let bridge = bridge::try_get()?;
    let c_class = CString::new(class_name).map_err(|e| format!("bad class: {e}"))?;
    let c_method = CString::new(method_name).map_err(|e| format!("bad method: {e}"))?;
    let handle = (bridge.harmony_patch_prefix)(c_class.as_ptr(), c_method.as_ptr(), prefix_fn);
    if handle.0 == 0 {
        return Err(format!(
            "harmony_patch_prefix({class_name}, {method_name}) failed"
        ));
    }
    Ok(Hook {
        handle,
        class_name: class_name.to_string(),
        method_name: method_name.to_string(),
        when: HookWhen::Prefix,
    })
}

/// Which object a `patch_prefix_ctx` callback receives as its
/// context handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookCtx {
    /// `__instance` of the patched (instance) method.
    Instance = 0,
    /// The method's first argument.
    Arg0 = 1,
}

/// Install a prefix patch whose callback receives a context
/// object (per `ctx`) as a FRESH handle in the `*const c_void`
/// parameter: cast the pointer value to i32 and wrap with
/// `MonoObject::from_handle` (Drop releases the handle; the
/// callback owns it). 0 means the context object was null.
/// Returning non-zero still skips the original method.
pub fn patch_prefix_ctx(
    class_name: &str,
    method_name: &str,
    ctx: HookCtx,
    prefix_fn: extern "C" fn(*const c_void) -> i32,
) -> Result<Hook, String> {
    let bridge = bridge::try_get()?;
    let c_class = CString::new(class_name).map_err(|e| format!("bad class: {e}"))?;
    let c_method = CString::new(method_name).map_err(|e| format!("bad method: {e}"))?;
    let handle =
        (bridge.harmony_patch_prefix_ctx)(c_class.as_ptr(), c_method.as_ptr(), ctx as i32, prefix_fn);
    if handle.0 == 0 {
        return Err(format!(
            "harmony_patch_prefix_ctx({class_name}, {method_name}, {ctx:?}) failed"
        ));
    }
    Ok(Hook {
        handle,
        class_name: class_name.to_string(),
        method_name: method_name.to_string(),
        when: HookWhen::Prefix,
    })
}

/// Install a postfix patch. The trampoline must be an `extern
/// "C" fn(*const c_void)`.
pub fn patch_postfix(
    class_name: &str,
    method_name: &str,
    postfix_fn: extern "C" fn(*const c_void),
) -> Result<Hook, String> {
    let bridge = bridge::try_get()?;
    let c_class = CString::new(class_name).map_err(|e| format!("bad class: {e}"))?;
    let c_method = CString::new(method_name).map_err(|e| format!("bad method: {e}"))?;
    let handle = (bridge.harmony_patch_postfix)(c_class.as_ptr(), c_method.as_ptr(), postfix_fn);
    if handle.0 == 0 {
        return Err(format!(
            "harmony_patch_postfix({class_name}, {method_name}) failed"
        ));
    }
    Ok(Hook {
        handle,
        class_name: class_name.to_string(),
        method_name: method_name.to_string(),
        when: HookWhen::Postfix,
    })
}

/// Workspace-standard registry for live hooks. Game crates push
/// here at worker init; `shutdown_all` releases them all on mod
/// teardown.
pub struct HookRegistry {
    entries: Mutex<Vec<Hook>>,
}

impl HookRegistry {
    pub const fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    pub fn register(&self, hook: Hook) {
        self.entries.lock().push(hook);
    }

    pub fn shutdown_all(&self) {
        let n = self.entries.lock().len();
        self.entries.lock().clear();
        if n > 0 {
            crate::mono::log(crate::mono::LogLevel::Info, &format!("unityforge: dropped {n} hook(s)"));
        }
    }

    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub static HOOK_REGISTRY: HookRegistry = HookRegistry::new();
