//! Load a save the way the Singleplayer menu does, and prove it
//! by the world number.
//!
//! `research_singleplayer_load` found three objects of class
//! `BP_HostNewGameServer_C` alive at the menu, each with its own
//! `LoadLevel` (research.md 26.9):
//!
//! ```text
//! BP_SingleplayerNewGameMenu
//! BP_HostNewGameServer
//! BP_HostLoadGameServer      <- what autoload called
//! ```
//!
//! Autoload used the host-a-server one and came up in a fresh
//! world every launch. This calls the singleplayer one instead,
//! with the load flag set, and reads the world number back.
//!
//! THE WORLD NUMBER IS THE PROOF. Map squares are named under
//! `WorldPresets` with a number in the path. A load brings back
//! the same number every time; a generated world has a new one
//! (5760, 244, 10776, 15820 across four launches).
//!
//! This STARTS A LEVEL LOAD, so it is `#[ignore]`d. Run it from
//! the main menu, deliberately:
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test load_singleplayer -- --ignored --test-threads=1 --nocapture
//! ```
//!
//! It never calls `DeleteExistingSave`, and it never touches the
//! two numbered buttons on a save row, because which of them
//! deletes is still unknown.

mod common;
use common::api_or_skip;
use serde_json::json;
use std::time::{Duration, Instant};

const GAME_INSTANCE: &str = "BP_SGKGameInstance_C";
const HOST_CLASS: &str = "BP_HostNewGameServer_C";

/// The singleplayer object, by name. Its two siblings host a
/// server; this is the one behind the Singleplayer button.
const SINGLEPLAYER: &str = "BP_SingleplayerNewGameMenu";

/// How long to wait for the world to come up before giving up.
const LOAD_WAIT: Duration = Duration::from_secs(180);

/// Set the load flag, call `LoadLevel` on the singleplayer
/// object, then report the world number.
#[test]
#[ignore = "starts a level load"]
fn load_through_the_singleplayer_object() {
    let Some(api) = api_or_skip() else { return };

    let gi = transient(&api, GAME_INSTANCE, None).expect("no live game instance");
    let host = transient(&api, HOST_CLASS, Some(SINGLEPLAYER))
        .unwrap_or_else(|| panic!("no live {SINGLEPLAYER}"));
    println!("game instance: {}", gi.full_name);
    println!("host object:   {}", host.full_name);

    // What is it about to load?
    let before = call(&api, GAME_INSTANCE, "SGK GetSaveGameSlotName", &gi.sel, 16);
    println!(
        "slot name: {:?}",
        modforge::client::read_fstring(&api, &before)
    );

    // Load a save rather than start a new game. LoadLevel is the
    // New Game path too, so this flag is the only thing that
    // separates them: set it, then read it back before calling.
    let set = api.op(
        "call",
        json!({
            "class": GAME_INSTANCE,
            "function": "SGK SetLoadSaveGame",
            "instance_selector": gi.sel,
            "parms_hex": "01",
        }),
    );
    assert!(set.ok, "SGK SetLoadSaveGame failed: {:?}", set.error);
    let back = call(&api, GAME_INSTANCE, "SGK GetLoadSaveGame", &gi.sel, 1);
    println!("load flag reads back: {back}");
    assert_eq!(
        back, "01",
        "the load flag did not take; not calling LoadLevel"
    );

    let go = api.op(
        "call",
        json!({
            "class": HOST_CLASS,
            "function": "LoadLevel",
            "instance_selector": host.sel,
            "parms_hex": "",
        }),
    );
    assert!(go.ok, "LoadLevel failed: {:?}", go.error);
    println!("LoadLevel called on {SINGLEPLAYER}");

    match wait_for_world(&api) {
        Some(square) => {
            println!("\nWORLD UP: {square}");
            println!(
                "\nRun this again from the menu. Same world number means it\n\
                 loaded the save; a different one means it generated a world."
            );
        }
        None => println!("\nno map square appeared within {LOAD_WAIT:?}"),
    }
}

/// A live object: its name and the selector to call it with.
struct Live {
    full_name: String,
    sel: String,
}

/// The `/Engine/Transient` instance of a class, optionally whose
/// name contains a fragment.
///
/// A class-chain search also returns the widget template inside
/// the `/Game/...WidgetTree` package. Calling a template returns
/// ok and does nothing, so the filter is not optional.
fn transient(api: &common::Api, class: &str, name_part: Option<&str>) -> Option<Live> {
    modforge::client::walk_class_chain_instances(api, class, 16)
        .into_iter()
        .find(|w| {
            w.full_name.contains("/Engine/Transient")
                && name_part.map(|p| w.full_name.contains(p)).unwrap_or(true)
        })
        .map(|w| Live {
            full_name: w.full_name,
            sel: w.addr_selector,
        })
}

/// Call a getter and return its parm block as hex.
fn call(api: &common::Api, class: &str, func: &str, sel: &str, bytes: usize) -> String {
    let r = api.op(
        "call",
        json!({
            "class": class,
            "function": func,
            "instance_selector": sel,
            "parms_hex": "0".repeat(bytes * 2),
        }),
    );
    assert!(r.ok, "{func} failed: {:?}", r.error);
    r.result["parms_hex_after"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

/// Wait for a map square to exist, and return its full name. The
/// world number is in that path.
///
/// The question is asked every three seconds THROUGH A LEVEL
/// LOAD, which is when the engine is destroying the menu and
/// building a world. `walk_class_chain` reads every object in the
/// game to answer it, so it runs on the game thread; asked from
/// the control plane's own thread it reads objects as they are
/// being freed and kills the process. That crash is what this
/// test caused on its first run, 2026-08-26 (research.md 26.6).
/// The routing is in `ueforge::ops::on_game_thread`.
fn wait_for_world(api: &common::Api) -> Option<String> {
    let start = Instant::now();
    while start.elapsed() < LOAD_WAIT {
        let r = api.op(
            "walk_class_chain",
            json!({ "needle": "BP_MasterAICharacter_C", "max": 8 }),
        );
        if r.ok {
            // Loud, because an answer that did NOT come from the
            // game thread is the unsafe walk this test is not
            // allowed to do.
            assert_ne!(
                r.result["game_thread"],
                json!(false),
                "walk_class_chain ran off the game thread; stop before it crashes"
            );
            let found = r.result["instances"]
                .as_array()
                .and_then(|a| a.iter().find_map(|i| i["full_name"].as_str()))
                .filter(|n| n.contains("WorldPresets"))
                .map(str::to_string);
            if let Some(name) = found {
                println!("world up after {:?}", start.elapsed());
                return Some(name);
            }
        }
        std::thread::sleep(Duration::from_secs(3));
    }
    None
}
