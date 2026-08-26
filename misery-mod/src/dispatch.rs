//! Game-thread dispatch.
//!
//! Hooks ProcessEvent on the player character class and drains
//! the job queue from the trampoline. The player character
//! receives ProcessEvent every frame while in a world, so queued
//! jobs run within a frame during play. At the main menu no
//! instance fires, so jobs wait (and time out) until a save is
//! loaded. See docs/research.md section 26.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use ueforge::pe_queue::DrainSite;

/// The one game-thread job queue for this mod. Worker threads
/// (ops, pollers) enqueue; the PE trampoline drains.
pub static DRAIN: DrainSite = DrainSite::new();

/// Trampoline fire count, for pe_stats.
static FIRES: AtomicU64 = AtomicU64::new(0);

/// The player character class receives ProcessEvent every frame
/// in play and resolves via find_class_fast (research.md 22.13
/// lists it as a class that works).
const HOOK_CLASS: &str = "BP_SGKMasterCharacter_C";

/// How long a queued job waits for the game thread before the
/// caller gets a timeout error. Shorter than the HTTP client's
/// 5s so the op returns an error instead of a dead socket.
const JOB_TIMEOUT: Duration = Duration::from_secs(3);

/// Register the pe_ping / pe_stats ops, then install the hook
/// (retrying until the class loads; the class only exists once
/// a save is loaded, so the backoff window is a day, not the
/// 10 minute default).
pub fn install() {
    register_ops();
    // The player class only exists once a save is loaded, and
    // features().once actions run inline and sequentially, so
    // this wait must not block the chain. A stoppable worker
    // retries every few seconds and stops itself once installed,
    // which also means shutdown can join it: a raw thread parked
    // in a day-long backoff would keep running after the DLL
    // unloaded and crash a hot reload.
    std::mem::forget(modforge::rpg::poller::spawn_interval(
        "misery-pe-dispatch",
        Duration::from_secs(2),
        try_install_hook,
    ));
}

/// One install attempt. Silent until it succeeds; once the hook
/// is in, later ticks do nothing.
fn try_install_hook() {
    if HOOK_INSTALLED.load(Ordering::Acquire) {
        return;
    }
    // find_class_fast resolves a stale reinstanced class for this
    // game (research.md 22.13): its CDO vtable never fires. Read
    // the vtable from the LIVE player instance instead.
    let Some(ptr) = ueforge::ue::actor::find_actors_by_chain(HOOK_CLASS)
        .into_iter()
        .next()
    else {
        return;
    };
    // SAFETY: ptr came from this call's GObjects iteration; it is
    // a live UObject.
    let obj = unsafe { &*(ptr as *const ueforge::ue::UObject) };
    match ueforge::hook::ProcessEventHook::install_for_object(HOOK_CLASS, obj, pe_handler) {
        Ok(h) => {
            HOOK_INSTALLED.store(true, Ordering::Release);
            ueforge::log::log(format_args!("pe_dispatch: hook installed on {HOOK_CLASS}"));
            ueforge::hook::register(h);
        }
        Err(e) => {
            ueforge::log::log(format_args!("pe_dispatch: install failed ({e}), will retry"));
        }
    }
}

static HOOK_INSTALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn pe_handler(
    this: &ueforge::ue::UObject,
    function: &ueforge::ue::UFunction,
    parms: *mut std::ffi::c_void,
    original: ueforge::hook::OriginalProcessEvent,
) {
    FIRES.fetch_add(1, Ordering::Relaxed);
    DRAIN.drain();
    // SAFETY: engine-supplied this/function/parms are forwarded
    // unchanged to the engine's original ProcessEvent.
    unsafe { original.call(this, function, parms) };
}

fn register_ops() {
    // The framework `call` op: UFunction invocation routed
    // through the game-thread queue. Anything invoked through it
    // actually executes now (research.md 26.1: the old direct
    // path called the wrong virtual).
    ueforge::debug::register_pe_call(
        &DRAIN,
        "misery: is a save loaded? the drain only fires in play",
        ueforge::selector::resolve,
    );
    ueforge::ops::OP_REGISTRY.register_many([
        ueforge::ops::OpDef::new(
            "pe_ping",
            "Run a no-op job on the game thread via the PE drain",
            "{}",
            |_args| {
                DRAIN.queue().enqueue(
                    || Ok(serde_json::json!({"game_thread": true})),
                    JOB_TIMEOUT,
                )
            },
        ),
        ueforge::ops::OpDef::new(
            "pe_stats",
            "PE dispatch counters",
            "{}",
            |_args| {
                Ok(serde_json::json!({
                    "hook_installed": !ueforge::hook::process_event::installed_defs().is_empty(),
                    "fires": FIRES.load(Ordering::Relaxed),
                    "drain_calls": DRAIN.drain_calls(),
                    "drained_cmds": DRAIN.drained_cmds(),
                    "queue_len": DRAIN.len(),
                    "peak": DRAIN.peak(),
                    "panics": ueforge::hook::process_event::panic_count_total(),
                }))
            },
        ),
    ]);
}
