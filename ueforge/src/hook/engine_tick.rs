//! `UEngine::Tick`: the game thread, with or without a world.
//!
//! A ProcessEvent hook only fires while the object it hangs off
//! is being called, so a hook on the player character is silent
//! at the main menu. `UEngine::Tick` runs once per frame on the
//! game thread for the life of the process, which is what makes
//! it the standard answer for menu-time work.
//!
//! UE4SS does the same thing for itself (`HookEngineTick = 1`,
//! `EngineTickResolveMethod = Scan` with a vtable fallback), but
//! exposes it only to Lua mods as `ExecuteInGameThread`. Its C++
//! mod API has no equivalent, so a Rust mod installs its own.
//!
//! This patches the vtable SLOT on the live `GameEngine` object.
//! UE4SS detours the Tick FUNCTION BODY, so the original this
//! captures routes through UE4SS's detour and the two chain
//! rather than recurse.
//!
//! The slot index is per engine version: 95 on UE 5.4. Read it
//! off the live object rather than assuming, and see misery
//! research.md 26.6 for how it was measured.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use parking_lot::Mutex;

use super::vtable;
use crate::ue::UObject;

/// `void UEngine::Tick(float DeltaSeconds, bool bIdleMode)`.
pub type EngineTickFn = unsafe extern "system" fn(*mut c_void, f32, bool);

/// Wrapper around the engine's own Tick.
#[derive(Clone, Copy)]
pub struct OriginalTick {
    f: EngineTickFn,
}

impl OriginalTick {
    /// # Safety
    /// `this` must be the engine pointer the trampoline was
    /// handed, and the arguments must be forwarded unchanged.
    pub unsafe fn call(&self, this: *mut c_void, delta: f32, idle: bool) {
        // SAFETY: `f` was read out of the vtable slot at install
        // time, so it has the engine's Tick ABI.
        unsafe { (self.f)(this, delta, idle) };
    }
}

type Handler = Box<dyn Fn(*mut c_void, f32, bool, OriginalTick) + Send + Sync>;

struct Installed {
    slot: *mut *mut c_void,
    original: EngineTickFn,
    handler: Handler,
}

// SAFETY: `slot` points into the engine's vtable, which outlives
// the process; the handler is Send + Sync by its bound. The
// pointer is only ever read, and only written at install time.
unsafe impl Send for Installed {}
unsafe impl Sync for Installed {}

static INSTALLED: Mutex<Option<&'static Installed>> = Mutex::new(None);
static FIRES: AtomicU64 = AtomicU64::new(0);
static THREAD_ID: AtomicU32 = AtomicU32::new(0);
static PANICS: AtomicUsize = AtomicUsize::new(0);

use std::sync::atomic::AtomicU32;

/// Times the engine has ticked through our trampoline.
pub fn fires() -> u64 {
    FIRES.load(Ordering::Relaxed)
}

/// OS thread the engine ticks on. This IS the game thread.
pub fn thread_id() -> u32 {
    THREAD_ID.load(Ordering::Relaxed)
}

/// Handler panics swallowed by the trampoline.
pub fn panics() -> usize {
    PANICS.load(Ordering::Relaxed)
}

pub fn is_installed() -> bool {
    INSTALLED.lock().is_some()
}

/// Read the Tick pointer out of `engine`'s vtable without
/// patching anything, so a caller can check the index against a
/// known address before installing.
///
/// # Safety
/// `engine` must be a live `UEngine`.
pub unsafe fn peek_slot(engine: &UObject, slot_idx: usize) -> Option<usize> {
    // SAFETY: every UE class on x86-64 starts with its vtable
    // pointer; offsets::uobject::VTABLE is 0.
    let vtable_ptr: *mut *mut c_void = unsafe {
        (engine as *const UObject as *const u8)
            .cast::<*mut *mut c_void>()
            .read_unaligned()
    };
    if vtable_ptr.is_null() {
        return None;
    }
    // SAFETY: slot_idx is within the class's vtable; the caller
    // supplies an index measured off this same object.
    let entry = unsafe { *vtable_ptr.add(slot_idx) };
    if entry.is_null() {
        None
    } else {
        Some(entry as usize)
    }
}

/// Patch `slot_idx` in `engine`'s vtable so `handler` runs every
/// frame on the game thread.
///
/// Installing twice is refused: the second install would capture
/// our own trampoline as the original and recurse forever.
///
/// # Safety
/// `engine` must be a live `UEngine`, and `slot_idx` must be its
/// Tick slot. A wrong index patches an unrelated virtual.
pub unsafe fn install<F>(engine: &UObject, slot_idx: usize, handler: F) -> Result<(), &'static str>
where
    F: Fn(*mut c_void, f32, bool, OriginalTick) + Send + Sync + 'static,
{
    let mut guard = INSTALLED.lock();
    if guard.is_some() {
        return Err("engine tick hook already installed");
    }
    // SAFETY: see peek_slot; the vtable pointer is the first
    // field of any UE object.
    let vtable_ptr: *mut *mut c_void = unsafe {
        (engine as *const UObject as *const u8)
            .cast::<*mut *mut c_void>()
            .read_unaligned()
    };
    if vtable_ptr.is_null() {
        return Err("engine vtable pointer is null");
    }
    // SAFETY: caller guarantees slot_idx is within the vtable.
    let slot = unsafe { vtable_ptr.add(slot_idx) };
    // SAFETY: the slot holds the engine's Tick function pointer.
    let original_raw = unsafe { *slot };
    if original_raw.is_null() {
        return Err("engine tick slot is null");
    }
    // SAFETY: original_raw is the engine's Tick, whose ABI is
    // EngineTickFn.
    let original: EngineTickFn = unsafe { std::mem::transmute(original_raw) };

    // Leaked for the same reason ProcessEventHook leaks its
    // entry: a tick already in flight may still hold a reference,
    // and freeing the handler across a DLL unload would leave a
    // vtable pointing into unloaded code.
    let entry: &'static Installed = Box::leak(Box::new(Installed {
        slot,
        original,
        handler: Box::new(handler),
    }));
    *guard = Some(entry);

    // SAFETY: write_slot handles the page-protection dance;
    // trampoline has the engine's Tick ABI.
    let prev = unsafe { vtable::write_slot(slot, trampoline as *mut c_void) };
    if prev.is_none() {
        *guard = None;
        return Err("VirtualProtect failed on the engine vtable");
    }
    Ok(())
}

/// Called by the engine every frame, on the game thread.
unsafe extern "system" fn trampoline(this: *mut c_void, delta: f32, idle: bool) {
    FIRES.fetch_add(1, Ordering::Relaxed);
    // SAFETY: no arguments, no memory access, cannot fail.
    THREAD_ID.store(
        unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() },
        Ordering::Relaxed,
    );
    let entry = { *INSTALLED.lock() };
    let Some(entry) = entry else {
        return;
    };
    let original = OriginalTick { f: entry.original };
    // A panic unwinding into the engine is undefined behaviour,
    // and this runs every frame, so one bad handler must not take
    // the process down.
    let handled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        (entry.handler)(this, delta, idle, original);
    }));
    if handled.is_err() {
        PANICS.fetch_add(1, Ordering::Relaxed);
        // The handler is responsible for calling the original. If
        // it panicked we cannot tell whether it got that far, and
        // skipping a frame's engine tick is safer than running it
        // twice.
    }
}
