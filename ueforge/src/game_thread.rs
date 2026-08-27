//! Keep a [`GameThread`] queue served, world or no world.
//!
//! A [`crate::pe_queue::GameThread`] queue only runs work when
//! something drains it, and mods have historically drained from
//! whatever gameplay hook they already had: a player character's
//! ProcessEvent, a kill multicast, a fall event. Those all stop
//! firing when the gameplay stops, so queued work starves at the
//! main menu, during loading, and any time the chosen event goes
//! quiet.
//!
//! `UEngine::Tick` runs once per frame on the game thread for the
//! entire life of the process and does not care whether a world
//! exists. Serving the queue from there removes the whole class
//! of problem. UE4SS does the same for itself
//! (`HookEngineTick = 1` in UE4SS-settings.ini) but exposes it
//! only to Lua mods, so a Rust mod installs its own.
//!
//! ```ignore
//! static PE_QUEUE: GameThread = GameThread::new();
//! ueforge::game_thread::serve(&PE_QUEUE);
//! ```
//!
//! Nothing here is game-specific: the engine object is found by
//! class-chain search, and Tick's vtable slot is derived by
//! resolving the function with patternsleuth and finding the slot
//! that holds it. If the scan fails, no hook is installed. A
//! guessed slot index would patch an unrelated virtual, which is
//! far worse than having no hook.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use parking_lot::Mutex;

use crate::hook::engine_tick::{self, OriginalTick};
use crate::pe_queue::GameThread;

/// Shorter than the HTTP client's timeout so a failed ping
/// returns a structured error instead of a dead connection.
const OP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// The engine object carrying `Tick`. `UGameEngine` in a packaged
/// game, matched by class chain so subclasses count.
const ENGINE_CLASS: &str = "GameEngine";

/// How far into the engine's vtable to look. A vtable carries no
/// length, so the search must be bounded rather than run until it
/// finds something. Tick sits at 95 on UE 5.4; the cap leaves
/// room for other versions without presuming the answer.
const VTABLE_MAX: usize = 400;

static QUEUE: Mutex<Option<&'static GameThread>> = Mutex::new(None);
static RESOLVE_FAILED: AtomicBool = AtomicBool::new(false);
static TICK_ADDR: AtomicUsize = AtomicUsize::new(0);
static TICK_SLOT: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Drain `queue` from `UEngine::Tick`, every frame, forever.
///
/// Safe to call before the engine exists: installation is retried
/// from UE4SS's per-frame callback until it succeeds. Call once,
/// at mod init.
pub fn serve(queue: &'static GameThread) {
    *QUEUE.lock() = Some(queue);
    crate::frame::on_update(try_install);
}

/// Register the standard game-thread call, ping, and status
/// operations for a queue served by [`serve`].
pub fn register_ops(queue: &'static GameThread, timeout_hint: &'static str) {
    crate::debug::register_pe_call(queue, timeout_hint, crate::selector::resolve);
    crate::ops::OP_REGISTRY.register_many([
        crate::ops::OpDef::new(
            "pe_ping",
            "Run a no-op job on the game thread",
            "{}",
            move |_args| {
                queue
                    .queue()
                    .enqueue(|| Ok(serde_json::json!({"game_thread": true})), OP_TIMEOUT)
            },
        ),
        crate::ops::OpDef::new(
            "pe_stats",
            "Game-thread dispatch counters",
            "{}",
            move |_args| {
                Ok(serde_json::json!({
                    "drain_calls": queue.drain_calls(),
                    "drained_cmds": queue.drained_cmds(),
                    "queue_len": queue.len(),
                    "peak": queue.peak(),
                    "frames": crate::frame::frames(),
                    "frame_thread": crate::frame::thread_id(),
                    "game_thread": status(),
                }))
            },
        ),
    ]);
}

/// True when the caller is already on the game thread.
///
/// `UEngine::Tick` only ever runs there, so the thread it last
/// ran on IS the game thread. Before the first tick this returns
/// false, which is the safe answer: work gets queued rather than
/// run in place.
pub fn is_game_thread() -> bool {
    let tick = engine_tick::thread_id();
    tick != 0 && tick == crate::frame::current_thread_id()
}

/// Run `f` on the game thread and wait for the result.
///
/// Anything that reads the game's object list, or touches any
/// live object, must go through here when it is called from a
/// background thread. A level unload deletes those objects, and
/// a background thread reading them at that moment reads freed
/// memory and kills the process. That is not theoretical: it is
/// what a crash dump showed, faulting inside
/// `ue::actor::find_objects_by_chain` one second after a level
/// started unloading.
///
/// Called from the game thread already, `f` runs immediately.
/// Queueing it there instead would wait for a drain that cannot
/// happen until the current call returns, which is a deadlock.
///
/// Fails if the queue is not being served yet, rather than
/// running `f` anyway.
pub fn run<F>(f: F, timeout: std::time::Duration) -> Result<serde_json::Value, String>
where
    F: FnOnce() -> Result<serde_json::Value, String> + Send + 'static,
{
    if is_game_thread() {
        return f();
    }
    let queue = *QUEUE.lock();
    let Some(q) = queue else {
        return Err("game_thread::run: no queue is being served".into());
    };
    q.queue().enqueue(f, timeout)
}

/// Wrap a background check so its whole body runs on the game
/// thread.
///
/// Written for the repeating checks a mod runs on a timer, which
/// all read live game objects and so must not run on their own
/// thread. Pass the result straight to the poller:
///
/// ```ignore
/// poller::spawn_interval("name", POLL, game_thread::each_tick(watcher));
/// ```
///
/// A tick that cannot reach the game thread is skipped, not run
/// anyway. Skipping a check costs a few seconds; reading a
/// deleted object costs the process.
pub fn each_tick<F>(tick: F) -> impl Fn() + Send + Sync + 'static
where
    F: Fn() + Send + Sync + 'static,
{
    let tick = std::sync::Arc::new(tick);
    move || {
        let t = tick.clone();
        let _ = run(
            move || {
                t();
                Ok(serde_json::Value::Null)
            },
            std::time::Duration::from_secs(5),
        );
    }
}

/// Absolute address patternsleuth resolved `UEngine::Tick` to, or
/// 0 before it has run.
pub fn tick_addr() -> usize {
    TICK_ADDR.load(Ordering::Relaxed)
}

/// Vtable slot the hook was installed into, or `None` before
/// install.
pub fn tick_slot() -> Option<usize> {
    match TICK_SLOT.load(Ordering::Relaxed) {
        usize::MAX => None,
        n => Some(n),
    }
}

/// True once the scan has failed and installation was abandoned.
pub fn resolve_failed() -> bool {
    RESOLVE_FAILED.load(Ordering::Relaxed)
}

/// Everything a snapshot or debug op wants to know about the
/// game-thread hook.
pub fn status() -> serde_json::Value {
    serde_json::json!({
        "installed": engine_tick::is_installed(),
        "fires": engine_tick::fires(),
        "thread": engine_tick::thread_id(),
        "panics": engine_tick::panics(),
        "tick_addr": format!("0x{:X}", tick_addr()),
        "tick_slot": tick_slot(),
        "resolve_failed": resolve_failed(),
    })
}

/// One install attempt.
///
/// Runs every frame until it succeeds, so both expensive steps
/// are guarded: the patternsleuth scan is a full pass over the
/// exe's `.text`, and a scan that fails once will fail again.
fn try_install() {
    if engine_tick::is_installed() || RESOLVE_FAILED.load(Ordering::Acquire) {
        return;
    }
    let Some(ptr) = crate::ue::actor::find_objects_by_chain(ENGINE_CLASS)
        .into_iter()
        .find(|p| {
            // SAFETY: p came from that call's GObjects iteration.
            let obj = unsafe { &*(*p as *const crate::ue::UObject) };
            obj.full_name().contains("/Engine/Transient")
        })
    else {
        // The engine object is not up yet. Not a failure; try
        // again next frame.
        return;
    };
    // SAFETY: ptr came from the GObjects iteration above.
    let engine = unsafe { &*(ptr as *const crate::ue::UObject) };

    let addr = match crate::ue::resolvers::resolve_game_engine_tick() {
        Ok(a) => a,
        Err(e) => {
            crate::log::log(format_args!("game_thread: {e}; engine tick hook skipped"));
            RESOLVE_FAILED.store(true, Ordering::Release);
            return;
        }
    };
    TICK_ADDR.store(addr, Ordering::Relaxed);

    // SAFETY: engine is the live UEngine; the search is bounded
    // by VTABLE_MAX.
    let Some(slot) = (unsafe { engine_tick::find_slot(engine, addr, VTABLE_MAX) }) else {
        crate::log::log(format_args!(
            "game_thread: {ENGINE_CLASS}::Tick at 0x{addr:X} is not in the first \
             {VTABLE_MAX} vtable slots; engine tick hook skipped"
        ));
        RESOLVE_FAILED.store(true, Ordering::Release);
        return;
    };

    // SAFETY: engine is the live UEngine, and slot was found by
    // matching the resolved Tick address in its own vtable.
    match unsafe { engine_tick::install(engine, slot, on_tick) } {
        Ok(()) => {
            TICK_SLOT.store(slot, Ordering::Relaxed);
            crate::log::log(format_args!(
                "game_thread: serving from {ENGINE_CLASS}::Tick at 0x{addr:X} (vtable slot {slot})"
            ));
        }
        Err(e) => {
            crate::log::log(format_args!(
                "game_thread: engine tick install failed ({e})"
            ));
            RESOLVE_FAILED.store(true, Ordering::Release);
        }
    }
}

/// Game thread, once per frame.
fn on_tick(this: *mut c_void, delta: f32, idle: bool, original: OriginalTick) {
    if let Some(q) = *QUEUE.lock() {
        q.drain();
    }
    // SAFETY: engine-supplied arguments forwarded unchanged.
    unsafe { original.call(this, delta, idle) };
}
