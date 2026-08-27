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

use ueforge::pe_queue::GameThread;

/// The one game-thread job queue for this mod. Worker threads
/// (ops, pollers) enqueue; the engine tick drains.
pub static DRAIN: GameThread = GameThread::new();

/// Register the pe_ping / pe_stats ops and start serving the
/// queue.
/// Stays here because it wires the shared Ueforge dispatcher into MISERY's operations.
pub fn install() {
    ueforge::game_thread::register_ops(
        &DRAIN,
        "misery: is the UEngine::Tick hook installed? see pe_stats.game_thread",
    );
    ueforge::game_thread::serve(&DRAIN);
}
