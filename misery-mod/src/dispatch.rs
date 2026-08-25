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
    // The backoff blocks until the player class loads (a save is
    // loaded), and features().once actions run inline and
    // sequentially, so the wait MUST live on its own thread or
    // every feature after this one stalls at the main menu.
    let _ = std::thread::Builder::new()
        .name("misery-pe-dispatch".into())
        .spawn(|| {
            let policy = ueforge::hook::RetryPolicy::new(
                Duration::from_millis(500),
                Duration::from_secs(5),
                Duration::from_secs(86400),
            );
            // find_class_fast resolves a stale reinstanced class
            // for this game (research.md 22.13): its CDO vtable
            // never fires. Read the vtable from the LIVE player
            // instance instead, which only exists once a save is
            // loaded; the backoff waits for it.
            let Some(h) = ueforge::hook::install_with_backoff("pe_dispatch", policy, || {
                let ptr = ueforge::ue::actor::find_actors_by_chain(HOOK_CLASS)
                    .into_iter()
                    .next()
                    .ok_or("player not loaded")?;
                // SAFETY: ptr came from the GObjects iteration
                // inside find_actors_by_chain this attempt; it is
                // a live UObject.
                let obj = unsafe { &*(ptr as *const ueforge::ue::UObject) };
                ueforge::hook::ProcessEventHook::install_for_object(HOOK_CLASS, obj, pe_handler)
            }) else {
                return;
            };
            ueforge::log::log(format_args!(
                "pe_dispatch: hook installed on {HOOK_CLASS}"
            ));
            ueforge::hook::register(h);
        });
}

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
