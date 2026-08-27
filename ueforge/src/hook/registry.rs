//! `HookRegistry`. The workspace-standard `<Subject>Registry`
//! wrapper for installed PE hooks. Per
//! [architecture.md](../../docs/architecture.md), every subsystem
//! exposes the same surface shape: a `<Subject>Def` record (here
//! [`HookDef`]) + a `<Subject>Registry` wrapper with `register` /
//! `def` / `entries` / `shutdown` operations.
//!
//! Hooks are the special case the architecture doc carves out:
//! handlers are user closures, so `HookDef`s are populated
//! imperatively at install time, not declared as a static `const`
//! the way `SkillDef`s are. The registry surface is the same; the
//! contents are produced at runtime instead of compile time.
//!
//! ## Why a registry exists
//!
//! Game crates used to `mem::forget` hook handles to keep them
//! alive past `worker()` thread exit. That broke hot-reload: the
//! handles' `Drop` impl (which restores vtable slots + drains
//! in-flight trampolines) never ran, so the next image's
//! `LoadLibraryExW` landed on a ghost trampoline pointing into
//! freed code.
//!
//! `HookRegistry` owns the handles in a `static`, so they survive
//! the init thread but stay reachable for orderly teardown.
//! [`HookRegistry::shutdown_all`] is wired into the framework's
//! `ueforge_mod_shutdown` (see `mod_main.rs`) and runs after the
//! game's `on_shutdown` callback returns. It:
//!
//! 1. Sets `process_event::SHUTTING_DOWN = true` so any new
//!    trampoline fires forward to the engine's original
//!    ProcessEvent without touching our handler closures.
//! 2. Drops every registered `ProcessEventHook`. Each Drop
//!    restores its class's vtable slot to the engine's
//!    original + waits up to 500ms for in-flight trampolines on
//!    that hook to drain.
//!
//! After this returns, no code in our DLL is on a thread's stack
//! via a PE trampoline; UE4SS can FreeLibrary safely.

use std::sync::atomic::Ordering;

use parking_lot::Mutex;

use super::process_event::{
    HookDef, ProcessEventHook, SHUTTING_DOWN, installed_defs, leaked_entry_count,
    panic_count_total,
};

/// The `<Subject>Registry` for the hook subsystem. Owns the
/// `ProcessEventHook` handles that keep installed vtable patches
/// alive, exposes telemetry accessors, and drives the hot-reload
/// teardown sequence.
pub struct HookRegistry {
    handles: Mutex<Vec<ProcessEventHook>>,
}

impl HookRegistry {
    pub const fn new() -> Self {
        Self {
            handles: Mutex::new(Vec::new()),
        }
    }

    /// Take ownership of one hook handle. The handle stays alive
    /// in the registry for the lifetime of the process (or until
    /// [`Self::shutdown_all`] runs at hot-reload).
    pub fn register(&self, hook: ProcessEventHook) {
        self.handles.lock().push(hook);
    }

    /// Take ownership of many handles in one shot. E.g. the
    /// list returned by `ProcessEventHook::install_many` for
    /// multi-class installs (player BP subclasses).
    pub fn register_many<I: IntoIterator<Item = ProcessEventHook>>(&self, hooks: I) {
        let mut g = self.handles.lock();
        for h in hooks {
            g.push(h);
        }
    }

    /// Take one hook back out and uninstall it.
    ///
    /// Dropping the handle restores the engine's original slot and
    /// waits for any call already inside our code to finish, so a
    /// hook that has done its job can be removed rather than left
    /// firing for the rest of the session.
    ///
    /// NEVER call this from inside the hook's own handler. The
    /// drop waits for in-flight calls to leave, and the caller
    /// would be one of them, waiting for itself.
    ///
    /// Returns true if a hook for that class was found.
    pub fn remove(&self, class_name: &str) -> bool {
        // Take it out under the lock, drop it outside, so the
        // registry is not held while waiting for calls to drain.
        let taken = {
            let mut g = self.handles.lock();
            g.iter()
                .position(|h| h.class_name() == class_name)
                .map(|i| g.remove(i))
        };
        match taken {
            Some(h) => {
                drop(h);
                crate::log!("hook: removed {class_name}");
                true
            }
            None => false,
        }
    }

    /// Drop every registered handle in registration order, after
    /// flipping the global `SHUTTING_DOWN` flag. Idempotent; safe
    /// to call when no hooks were ever installed.
    pub fn shutdown_all(&self) {
        SHUTTING_DOWN.store(true, Ordering::Release);
        // Take handles out before iterating so we don't hold the
        // registry lock while waiting for trampoline drains.
        let handles: Vec<ProcessEventHook> = std::mem::take(&mut *self.handles.lock());
        let n = handles.len();
        drop(handles);
        if n > 0 {
            crate::log!("hook: shutdown_all dropped {n} hook(s)");
        }
    }

    /// Snapshot the currently-installed `HookDef`s. Read-only
    /// view for debug endpoints.
    pub fn defs(&self) -> Vec<&'static HookDef> {
        installed_defs()
    }

    /// Sum of trampoline-caught handler panics across every
    /// currently-installed hook. Nonzero = some handler is
    /// silently failing.
    pub fn panic_count_total(&self) -> u64 {
        panic_count_total()
    }

    /// Cumulative count of `HookDef` allocations made by any
    /// hook install in this process. Per the audit in hooks.md,
    /// each install `Box::leak`s one record that is never freed
    /// (re-using across DLL unloads is unsafe). Surface this in
    /// dev sessions to monitor accumulation across hot-reloads.
    pub fn leaked_entry_count(&self) -> u64 {
        leaked_entry_count()
    }

    /// Number of handles the registry currently owns. Equal to
    /// `defs().len()` while the registry is healthy; diverges
    /// only during/after `shutdown_all`.
    pub fn len(&self) -> usize {
        self.handles.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.handles.lock().is_empty()
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-wide `HookRegistry` singleton. Free-function helpers
/// below ([`register`] / [`shutdown_all`] / etc.) are thin
/// wrappers that delegate to this instance, kept for back-compat
/// with the pre-`HookRegistry` API.
pub static HOOK_REGISTRY: HookRegistry = HookRegistry::new();

/// Register one hook with the global teardown registry. Thin
/// wrapper over [`HookRegistry::register`]. Prefer the method
/// in new code.
pub fn register(hook: ProcessEventHook) {
    HOOK_REGISTRY.register(hook);
}

/// Uninstall one hook by the class it was installed for. Thin
/// wrapper over [`HookRegistry::remove`], including its rule:
/// never from inside that hook's own handler.
pub fn remove(class_name: &str) -> bool {
    HOOK_REGISTRY.remove(class_name)
}

/// Register many hooks (e.g. the multi-class handle list returned
/// by `ProcessEventHook::install_many`). Thin wrapper over
/// [`HookRegistry::register_many`].
pub fn register_many<I: IntoIterator<Item = ProcessEventHook>>(hooks: I) {
    HOOK_REGISTRY.register_many(hooks);
}

/// Drop every registered hook + flip the global `SHUTTING_DOWN`
/// flag. Thin wrapper over [`HookRegistry::shutdown_all`]. Called
/// by the framework's shutdown path.
pub fn shutdown_all() {
    HOOK_REGISTRY.shutdown_all();
}
