//! Generic function-call detour: install a Rust handler in front of
//! any address in the host process, with the original function still
//! reachable through `Hook::call_original`. Built on top of
//! [`retour::GenericDetour`].
//!
//! # Why
//!
//! Every modforge consumer ends up writing the same boilerplate to
//! install a function hook:
//!
//! - declare `static D: AtomicPtr<GenericDetour<Fn>> = ...`
//! - in `arm()`, build the detour, `enable()`, store the ptr
//! - in `disarm()`, load+disable+drop
//! - the handler reads the static, calls `(*p).call(args)`
//! - if vanilla crashes, the entire host goes down
//!
//! This module collapses all of that into a single struct, registers
//! installed hooks for diagnostics, and wraps `call_original` in
//! [`crate::seh::guard`] so a faulting vanilla function returns
//! `Err(SehError)` instead of taking the process down.
//!
//! # Example
//!
//! ```ignore
//! use modforge::hook::Hook;
//!
//! type GeneEffectFn = unsafe extern "system" fn(*mut f32, *mut c_void);
//!
//! unsafe extern "system" fn my_handler(buf: *mut f32, ctx: *mut c_void) {
//!     // call vanilla first
//!     let _ = HOOK.with(|h| h.call_original(buf, ctx));
//!     // post-process buf
//! }
//!
//! static HOOK: OnceCell<Hook<GeneEffectFn>> = OnceCell::new();
//!
//! HOOK.set(Hook::install("gene_effect", target_addr, my_handler)?).ok();
//! ```

use std::sync::Mutex;

use retour::{Function, GenericDetour};

/// One installed hook on a single function. Drop disables.
pub struct Hook<F: Function> {
    detour: GenericDetour<F>,
    name: &'static str,
    target_addr: usize,
}

impl<F: Function> Hook<F> {
    /// Install a detour. `target_addr` is the absolute runtime address
    /// (image_base + RVA) of the function to hook. `replacement` is
    /// the Rust handler with the same calling convention as the target.
    ///
    /// # Safety
    /// - `target_addr` must point to a function whose ABI matches `F`.
    /// - The function's machine code must be patchable in-place (retour
    ///   relocates the first few bytes; this fails on tiny functions).
    /// - `replacement` must remain valid for the lifetime of the Hook.
    pub unsafe fn install(
        name: &'static str,
        target_addr: usize,
        replacement: F,
    ) -> anyhow::Result<Self> {
        let target: F = unsafe { std::mem::transmute_copy(&target_addr) };
        // SAFETY: caller ensures target_addr matches F's ABI and points
        // to patchable code. retour copies the prologue + writes a jmp.
        let detour = unsafe { GenericDetour::new(target, replacement) }
            .map_err(|e| anyhow::anyhow!("retour::new for '{name}' at 0x{target_addr:x}: {e}"))?;
        // SAFETY: enables the detour we just constructed.
        unsafe { detour.enable() }.map_err(|e| {
            anyhow::anyhow!("retour::enable for '{name}' at 0x{target_addr:x}: {e}")
        })?;
        let h = Hook {
            detour,
            name,
            target_addr,
        };
        REGISTRY.lock().unwrap().push(HookInfo {
            name,
            target_addr,
            enabled: true,
        });
        Ok(h)
    }

    /// Diagnostic name passed to [`install`].
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Target runtime address.
    pub fn target_addr(&self) -> usize {
        self.target_addr
    }

    /// Access the underlying [`GenericDetour`]. Use the macro
    /// [`call_original!`] instead of touching this directly when
    /// SEH protection is desired.
    pub fn detour(&self) -> &GenericDetour<F> {
        &self.detour
    }
}

impl<F: Function> Drop for Hook<F> {
    fn drop(&mut self) {
        // SAFETY: disabling a detour we own.
        let _ = unsafe { self.detour.disable() };
        let mut reg = REGISTRY.lock().unwrap();
        if let Some(idx) = reg.iter().position(|h| h.target_addr == self.target_addr) {
            reg.swap_remove(idx);
        }
    }
}

/// One row in the diagnostic registry.
#[derive(Debug, Clone)]
pub struct HookInfo {
    pub name: &'static str,
    pub target_addr: usize,
    pub enabled: bool,
}

static REGISTRY: Mutex<Vec<HookInfo>> = Mutex::new(Vec::new());

/// List every currently-installed [`Hook`]. Used by HTTP diagnostics
/// (e.g. `hooks.list` op) to surface what's hooked at runtime.
pub fn registry() -> Vec<HookInfo> {
    REGISTRY.lock().unwrap().clone()
}

/// Macro to call a hook's original function inside [`crate::seh::guard`].
///
/// Without the SEH wrap, any access violation inside vanilla code
/// kills the host process. With it, the call returns `Err(SehError)`
/// and the host stays alive.
///
/// # Example
/// ```ignore
/// let r = modforge::call_original!(hook, arg1, arg2);
/// match r {
///     Ok(value) => use_value(value),
///     Err(e)    => log::warn!("vanilla crashed: {e}"),
/// }
/// ```
#[macro_export]
macro_rules! call_original {
    ($hook:expr $(, $arg:expr )* $(,)?) => {{
        let __hook = &$hook;
        $crate::seh::guard(|| unsafe { __hook.detour().call($($arg),*) })
    }};
}
