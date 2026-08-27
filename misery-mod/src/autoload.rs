//! Load the last save automatically, so a restart lands in game.
//!
//! The menu path a player clicks is Singleplayer, Load Game, then
//! the save. Underneath, the Survival Game Kit stores the choice
//! on the game instance and then opens the level:
//!
//! ```text
//! SGK SetSaveGameSlotName(FString)   which slot
//! SGK SetLoadSaveGame(bool)          load it, don't start new
//! LoadLevel()                        on BP_SingleplayerNewGameMenu
//! ```
//!
//! The game instance already holds the slot name at startup
//! ("Save 1" here), so no FString has to be constructed: read it
//! back, check it, and load.
//!
//! Every step is guarded, because `LoadLevel` is ALSO the New Game
//! path and a wrong turn here would start a fresh game over the
//! player's save:
//!
//!   - no slot name held  -> do nothing
//!   - `FindExistingSave` says the save is missing -> do nothing
//!   - the load flag does not read back as set -> do nothing
//!
//! All of it runs on the game thread through
//! `ueforge::game_thread`, because UFunction calls made from any
//! other thread appear to work and then crash the game
//! (research.md 26.6).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ueforge::ue;

const GAME_INSTANCE: &str = "BP_SGKGameInstance_C";

/// Three objects share this class, and only the object name
/// tells them apart (research.md 26.9):
///
/// ```text
/// BP_SingleplayerNewGameMenu
/// BP_HostNewGameServer
/// BP_HostLoadGameServer
/// ```
const HOST_CLASS: &str = "BP_HostNewGameServer_C";

/// The object behind the Singleplayer button, and the one to
/// call `LoadLevel` on.
///
/// This used to be `BP_HostLoadGameServer`, which hosts a server
/// and generates a world: every launch came up somewhere the
/// player had never been. Calling the singleplayer object instead
/// loaded the operator's own save, proven live on 2026-08-26 by
/// the emission count coming back as 42 rather than 1
/// (`tests/load_singleplayer.rs`).
const SINGLEPLAYER: &str = "BP_SingleplayerNewGameMenu";

/// `FindExistingSave` takes an FString and returns a bool. The
/// bool lands in byte 16, measured live: an existing slot came
/// back `...0800000001`, a null slot `...0000000000`
/// (`research_load::find_existing_save_layout`).
const FIND_PARMS: usize = 17;
const FIND_RESULT_BYTE: usize = 16;

/// An `FString` parm block: `{ TCHAR* Data; int32 Num; int32 Max; }`.
const FSTRING_PARMS: usize = 16;

/// Set once the attempt has resolved, one way or the other. This
/// runs exactly once per launch: retrying a load is not something
/// to do behind the player's back.
static SETTLED: AtomicBool = AtomicBool::new(false);

/// Starts checking for the player's last save so MISERY can load it automatically.
/// Stays here because automatic loading is mod policy built around MISERY's save menu Blueprints.
pub fn install() {
    std::mem::forget(modforge::rpg::poller::spawn_interval(
        "misery-autoload",
        Duration::from_secs(2),
        tick,
    ));
}

/// Worker thread: hand the work to the game thread and act on
/// what it reports.
/// Stays here because its retry and logging policy belongs to MISERY's autoload feature.
fn tick() {
    if SETTLED.load(Ordering::Acquire) {
        return;
    }
    let outcome = crate::dispatch::DRAIN
        .queue()
        .enqueue(|| Ok(serde_json::json!(attempt())), Duration::from_secs(5));
    let Ok(v) = outcome else {
        // The game thread is not serving yet. Not a failure.
        return;
    };
    let Some(msg) = v.as_str() else { return };
    if msg == WAITING {
        return;
    }
    SETTLED.store(true, Ordering::Release);
    ueforge::log::log(format_args!("autoload: {msg}"));
}

/// Returned while the menu has not finished building. Not an
/// outcome, so the poller keeps trying.
const WAITING: &str = "waiting";

/// Game thread. Returns what happened, as a sentence for the log.
/// Stays here because it follows MISERY's exact save-slot flags and load-screen functions.
fn attempt() -> String {
    let Some(gi) = ue::actor::find_transient_object(GAME_INSTANCE, None) else {
        return WAITING.to_string();
    };
    let Some(host) = ue::actor::find_transient_object(HOST_CLASS, Some(SINGLEPLAYER)) else {
        return WAITING.to_string();
    };

    // Which slot does the game already intend to load?
    let mut slot = [0u8; FSTRING_PARMS];
    if let Err(e) = unsafe {
        ue::pe_call::call_ufunction_bytes(gi, GAME_INSTANCE, "SGK GetSaveGameSlotName", &mut slot)
    } {
        return format!("could not read the slot name ({e})");
    }
    let num = i32::from_le_bytes([slot[8], slot[9], slot[10], slot[11]]);
    let ptr = u64::from_le_bytes(slot[0..8].try_into().unwrap_or_default());
    if ptr == 0 || num <= 0 {
        return "no save slot is set; nothing to load".to_string();
    }

    // Does that save actually exist? This is what keeps a missing
    // save from turning into a new game.
    let mut find = [0u8; FIND_PARMS];
    find[..FSTRING_PARMS].copy_from_slice(&slot);
    if let Err(e) = unsafe {
        ue::pe_call::call_ufunction_bytes(host, HOST_CLASS, "FindExistingSave", &mut find)
    } {
        return format!("could not check whether the save exists ({e})");
    }
    if find[FIND_RESULT_BYTE] == 0 {
        return "the save the game points at does not exist; not loading".to_string();
    }

    // Load, rather than start a new game. Read it back: LoadLevel
    // is the New Game path too, and this flag is the only thing
    // that separates them.
    let mut on = [1u8];
    if let Err(e) = unsafe {
        ue::pe_call::call_ufunction_bytes(gi, GAME_INSTANCE, "SGK SetLoadSaveGame", &mut on)
    } {
        return format!("could not set the load flag ({e})");
    }
    let mut back = [0u8];
    if let Err(e) = unsafe {
        ue::pe_call::call_ufunction_bytes(gi, GAME_INSTANCE, "SGK GetLoadSaveGame", &mut back)
    } {
        return format!("could not read the load flag back ({e})");
    }
    if back[0] != 1 {
        return "the load flag did not take; not calling LoadLevel".to_string();
    }

    let mut none: [u8; 0] = [];
    if let Err(e) =
        unsafe { ue::pe_call::call_ufunction_bytes(host, HOST_CLASS, "LoadLevel", &mut none) }
    {
        return format!("LoadLevel failed ({e})");
    }
    "loading the saved game".to_string()
}
