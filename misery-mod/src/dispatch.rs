//! Game-thread dispatch.
//!
//! Hooks ProcessEvent on the player character class and drains
//! the job queue from the trampoline. The player character
//! receives ProcessEvent every frame while in a world, so queued
//! jobs run within a frame during play. At the main menu no
//! instance fires, so jobs wait (and time out) until a save is
//! loaded. See docs/research.md section 26.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use ueforge::pe_queue::GameThread;

/// The one game-thread job queue for this mod. Worker threads
/// (ops, pollers) enqueue; the PE trampoline drains.
pub static DRAIN: GameThread = GameThread::new();

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
    ueforge::frame::on_update(on_frame);
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
    HOOK_THREAD.store(ueforge::frame::current_thread_id(), Ordering::Relaxed);
    DRAIN.drain();
    // SAFETY: engine-supplied this/function/parms are forwarded
    // unchanged to the engine's original ProcessEvent.
    unsafe { original.call(this, function, parms) };
}

/// Thread the ProcessEvent hook last ran on. ProcessEvent is
/// game-thread only, so this IS the game thread, and comparing
/// it against `ueforge::frame::thread_id()` says whether UE4SS
/// calls `on_update` there too.
static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);

/// The engine object, and the vtable slot its `Tick` sits in.
///
/// Measured live off the vtable and cross-checked against the
/// address UE4SS resolves by patternsleuth scan: both agree
/// (research.md 26.6). The index is per engine version; 95 is
/// UE 5.4.
const ENGINE_CLASS: &str = "GameEngine";
const ENGINE_TICK_SLOT: usize = 95;

/// UE4SS's `on_update` fires every frame but on UE4SS's OWN
/// thread, not the game thread (measured: 508 vs 19556). It must
/// never drain the queue, because that would run queued
/// UFunction calls off the game thread, which is exactly the
/// undefined behaviour that crashed the game on 2026-08-26.
///
/// It is a fine place to keep trying to install the engine tick
/// hook, which is what actually serves the queue at the menu.
fn on_frame() {
    try_install_engine_tick();
}

/// Hook `UEngine::Tick`: the game thread, world or no world.
fn try_install_engine_tick() {
    if ueforge::hook::engine_tick::is_installed() {
        return;
    }
    let Some(ptr) = ueforge::ue::actor::find_objects_by_chain(ENGINE_CLASS)
        .into_iter()
        .find(|p| {
            // SAFETY: p came from that call's GObjects iteration.
            let obj = unsafe { &*(*p as *const ueforge::ue::UObject) };
            obj.full_name().contains("/Engine/Transient")
        })
    else {
        return;
    };
    // SAFETY: ptr came from the GObjects iteration above.
    let engine = unsafe { &*(ptr as *const ueforge::ue::UObject) };
    // SAFETY: engine is the live UEngine; the slot index was
    // measured off this same vtable.
    match unsafe { ueforge::hook::engine_tick::install(engine, ENGINE_TICK_SLOT, on_engine_tick) } {
        Ok(()) => ueforge::log::log(format_args!(
            "pe_dispatch: hooked {ENGINE_CLASS}::Tick (vtable slot {ENGINE_TICK_SLOT})"
        )),
        Err(e) => ueforge::log::log(format_args!("pe_dispatch: engine tick install failed ({e})")),
    }
}

/// Game thread, once per frame, menu included.
fn on_engine_tick(
    this: *mut std::ffi::c_void,
    delta: f32,
    idle: bool,
    original: ueforge::hook::engine_tick::OriginalTick,
) {
    DRAIN.drain();
    // SAFETY: engine-supplied arguments forwarded unchanged.
    unsafe { original.call(this, delta, idle) };
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
                    // UE4SS's per-frame callback: does it run,
                    // and is it the same thread ProcessEvent
                    // runs on?
                    "frames": ueforge::frame::frames(),
                    "frame_thread": ueforge::frame::thread_id(),
                    "hook_thread": HOOK_THREAD.load(Ordering::Relaxed),
                    // UEngine::Tick: the game thread with no
                    // world loaded.
                    "tick_installed": ueforge::hook::engine_tick::is_installed(),
                    "tick_fires": ueforge::hook::engine_tick::fires(),
                    "tick_thread": ueforge::hook::engine_tick::thread_id(),
                    "tick_panics": ueforge::hook::engine_tick::panics(),
                }))
            },
        ),
    ]);
}
