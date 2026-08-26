//! Game-thread dispatch.
//!
//! One job queue, served from `UEngine::Tick` via
//! `ueforge::game_thread`. Tick runs once per frame on the game
//! thread for the life of the process, so queued work runs at the
//! main menu, during loading, and in play alike.
//!
//! This used to hang off a ProcessEvent hook on the player
//! character, which only exists once a save is loaded. Work
//! queued at the menu timed out, which is what blocked loading a
//! save from the mod. See docs/research.md 26.6.

use std::time::Duration;

use ueforge::pe_queue::GameThread;

/// The one game-thread job queue for this mod. Worker threads
/// (ops, pollers) enqueue; the engine tick drains.
pub static DRAIN: GameThread = GameThread::new();

/// How long a queued job waits for the game thread before the
/// caller gets a timeout error. Shorter than the HTTP client's
/// 5s so the op returns an error instead of a dead socket.
const JOB_TIMEOUT: Duration = Duration::from_secs(3);

/// Register the pe_ping / pe_stats ops and start serving the
/// queue.
pub fn install() {
    register_ops();
    ueforge::game_thread::serve(&DRAIN);
}

fn register_ops() {
    // The framework `call` op: UFunction invocation routed
    // through the game-thread queue. Anything invoked through it
    // actually executes now (research.md 26.1: the old direct
    // path called the wrong virtual).
    ueforge::debug::register_pe_call(
        &DRAIN,
        "misery: is the UEngine::Tick hook installed? see pe_stats.game_thread",
        ueforge::selector::resolve,
    );
    ueforge::ops::OP_REGISTRY.register_many([
        ueforge::ops::OpDef::new(
            "pe_ping",
            "Run a no-op job on the game thread",
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
            "Game-thread dispatch counters",
            "{}",
            |_args| {
                Ok(serde_json::json!({
                    "drain_calls": DRAIN.drain_calls(),
                    "drained_cmds": DRAIN.drained_cmds(),
                    "queue_len": DRAIN.len(),
                    "peak": DRAIN.peak(),
                    // UE4SS's per-frame callback. Fires every
                    // frame but on UE4SS's OWN thread, so it is
                    // used to install the tick hook and for
                    // nothing that enters the engine.
                    "frames": ueforge::frame::frames(),
                    "frame_thread": ueforge::frame::thread_id(),
                    // UEngine::Tick: the game thread, world or
                    // no world. This is what serves the queue.
                    "game_thread": ueforge::game_thread::status(),
                }))
            },
        ),
    ]);
}
