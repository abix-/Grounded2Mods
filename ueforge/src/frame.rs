//! UE4SS's per-frame callback.
//!
//! `RC::CppUserModBase` declares `on_update()`, which UE4SS calls
//! every frame for the whole life of the process. Unlike a
//! ProcessEvent hook on a gameplay object, it does not care
//! whether a world is loaded, so it fires on the main menu, in
//! loading screens, and in play.
//!
//! That makes it the right place to flush a game-thread queue. A
//! hook on the player character only fires once a save is
//! loaded, which is why menu-time work timed out before.
//!
//! Whether UE4SS calls `on_update` on the game thread is not
//! documented here; `thread_id()` records the calling thread so
//! a test can compare it against the thread a ProcessEvent hook
//! runs on.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;

/// Called every frame from UE4SS's `on_update`.
pub type FrameFn = fn();

static CALLBACKS: Mutex<Vec<FrameFn>> = Mutex::new(Vec::new());
static FRAMES: AtomicU64 = AtomicU64::new(0);
static THREAD_ID: AtomicU32 = AtomicU32::new(0);

/// Run `f` once per frame. Registration order is call order.
pub fn on_update(f: FrameFn) {
    if let Ok(mut v) = CALLBACKS.lock() {
        v.push(f);
    }
}

/// Frames seen since the mod loaded. Zero means UE4SS is not
/// calling `on_update` at all.
pub fn frames() -> u64 {
    FRAMES.load(Ordering::Relaxed)
}

/// OS thread id UE4SS last called `on_update` on, 0 before the
/// first call. Compare against the thread a ProcessEvent hook
/// runs on to decide whether this is the game thread.
pub fn thread_id() -> u32 {
    THREAD_ID.load(Ordering::Relaxed)
}

/// The current OS thread id, for callers recording where their
/// own callbacks ran.
pub fn current_thread_id() -> u32 {
    // SAFETY: GetCurrentThreadId takes no arguments, touches no
    // memory, and cannot fail.
    unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() }
}

/// Entry point for the C++ shim's `on_update` override. Never
/// unwinds into C++.
pub fn run_update() {
    FRAMES.fetch_add(1, Ordering::Relaxed);
    THREAD_ID.store(current_thread_id(), Ordering::Relaxed);
    let Ok(callbacks) = CALLBACKS.lock() else {
        return;
    };
    for f in callbacks.iter() {
        let f = *f;
        // A panic crossing the FFI boundary is undefined
        // behaviour, so one bad callback must not take the
        // process with it.
        let _ = std::panic::catch_unwind(f);
    }
}
