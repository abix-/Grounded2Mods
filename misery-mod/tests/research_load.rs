//! Loading a save from the main menu, without touching the mouse.
//!
//! Same method that solved the playtest notice (research.md
//! 26.5): find the widget the game already uses, list its LIVE
//! functions, and call the one the player's own click calls.
//! The menu widgets are created after startup, so they are
//! absent from the discovery cache; `class_functions` reads them
//! off a live instance instead.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_load -- --test-threads=1 --nocapture
//! ```

mod common;
use common::api_or_skip;
use serde_json::json;

/// Menu widgets seen live on the main menu, outermost first,
/// plus the game instance: none of the menu classes carry a load
/// function, so the load itself lives further in.
const MENU_CLASSES: &[&str] = &[
    "BP_MainMenu_C",
    "BP_SingleplayerMenu_C",
    "BP_LoadGameMenu_C",
    "BP_LoadGameMenuPanel_C",
    "BP_SGKGameInstance_C",
    // Starting a game goes through the host-server widget: the
    // live tree has both a BP_HostNewGameServer and a
    // BP_HostLoadGameServer, both of this class.
    "BP_HostNewGameServer_C",
];

/// What can each menu widget be told to do?
#[test]
fn menu_class_functions() {
    let Some(api) = api_or_skip() else { return };
    for class in MENU_CLASSES {
        let r = api.op("class_functions", json!({ "class": class }));
        if !r.ok {
            println!("\n{class}: {:?}", r.error);
            continue;
        }
        let fns = r.result["functions"].as_array().cloned().unwrap_or_default();
        println!("\n{class} ({} functions)", fns.len());
        println!("  {}", r.result["full_name"].as_str().unwrap_or("?"));
        for f in &fns {
            println!(
                "  {:<60} parms={} bytes={}",
                f["name"].as_str().unwrap_or("?"),
                f["num_parms"],
                f["parms_size"]
            );
        }
    }
}

/// Properties of the live load menu, its panel, and the game
/// instance. The save rows are not UserWidgets, so the widget
/// walk cannot see them; their state has to come from whatever
/// holds it.
///
/// Read-only. `BP_LoadGameMenuPanel_C` owns `DeleteExistingSave`,
/// so nothing here calls anything on it.
#[test]
fn load_menu_state() {
    let Some(api) = api_or_skip() else { return };
    for class in ["BP_LoadGameMenu_C", "BP_LoadGameMenuPanel_C", "BP_SGKGameInstance_C"] {
        let live = modforge::client::walk_class_chain_instances(&api, class, 8);
        println!("\n=== {class}: {} live instance(s)", live.len());
        for w in &live {
            println!("\n-- {}", w.full_name);
            // inspect_address takes hex text, not the u64.
            let r = api.op("inspect_address", json!({"addr": format!("0x{:X}", w.addr)}));
            if r.ok {
                println!("{}", serde_json::to_string_pretty(&r.result).unwrap_or_default());
            } else {
                println!("inspect failed: {:?}", r.error);
            }
        }
    }
}

/// What does a save row store? Each row on the load screen is a
/// `BP_LoadGameMenuPanel_C`, and its two buttons are named only
/// by number (`Button_284`, `Button_372`). One loads and one
/// deletes. The field list names them and shows where the slot
/// name is kept.
///
/// Read-only.
#[test]
fn save_panel_fields() {
    let Some(api) = api_or_skip() else { return };
    for class in ["BP_LoadGameMenuPanel_C", "BP_LoadGameMenu_C"] {
        let r = api.op("walk_class", json!({ "class": class }));
        println!("\n=== {class}");
        if !r.ok {
            println!("walk_class failed: {:?}", r.error);
            continue;
        }
        println!("{}", serde_json::to_string_pretty(&r.result).unwrap_or_default());
    }
}

/// The game instance, whose two setters decide what the next
/// level load does.
const GAME_INSTANCE: &str = "BP_SGKGameInstance_C";

fn game_instance_selector(api: &common::Api) -> String {
    let live = modforge::client::walk_class_chain_instances(api, GAME_INSTANCE, 4);
    let inst = live
        .iter()
        .find(|w| w.full_name.contains("/Engine/Transient"))
        .expect("no live game instance");
    inst.addr_selector.clone()
}

/// Decode an `FString` parm block: `{ TCHAR* Data; int32 Num;
/// int32 Max; }`, UTF-16 characters.
fn read_fstring(api: &common::Api, parms_hex: &str) -> String {
    let bytes = hex_to_bytes(parms_hex);
    if bytes.len() < 16 {
        return String::new();
    }
    let ptr = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    let num = i32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if ptr == 0 || num <= 0 {
        return String::new();
    }
    let r = api.op(
        "read_bytes",
        json!({
            "instance_selector": format!("addr:0x{ptr:X}"),
            "offset": 0,
            "length": (num as usize) * 2,
        }),
    );
    if !r.ok {
        return format!("<read_bytes failed: {:?}>", r.error);
    }
    let raw = hex_to_bytes(r.result["bytes_hex"].as_str().unwrap_or(""));
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|c| *c != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

fn hex_to_bytes(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .filter_map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok())
        .collect()
}

/// What does the game currently think it should load? Read-only,
/// and the first proof that a Blueprint call actually executes
/// from the main menu.
#[test]
fn read_current_slot() {
    let Some(api) = api_or_skip() else { return };
    let selector = game_instance_selector(&api);
    println!("game instance: {selector}");

    // 1 parm, 1 byte: the bool that says "load a save rather
    // than start a new game".
    let flag = api.op(
        "call",
        json!({
            "class": GAME_INSTANCE,
            "function": "SGK GetLoadSaveGame",
            "instance_selector": selector,
            "parms_hex": "00",
        }),
    );
    assert!(flag.ok, "SGK GetLoadSaveGame failed: {:?}", flag.error);
    println!("SGK GetLoadSaveGame -> {}", flag.result);

    // 1 parm, 16 bytes: the FString slot name.
    let name = api.op(
        "call",
        json!({
            "class": GAME_INSTANCE,
            "function": "SGK GetSaveGameSlotName",
            "instance_selector": selector,
            "parms_hex": "0".repeat(32),
        }),
    );
    assert!(name.ok, "SGK GetSaveGameSlotName failed: {:?}", name.error);
    println!("SGK GetSaveGameSlotName raw -> {}", name.result);
    let hex = name.result["parms_hex_after"].as_str().unwrap_or("");
    println!("slot name: {:?}", read_fstring(&api, hex));
}

/// Where does `FindExistingSave` put its answer?
///
/// 2 parms, 17 bytes: an FString in and a bool out. Auto-load
/// needs that bool to skip cleanly when the save is missing, so
/// the byte it lands in has to be measured, not assumed. Ask it
/// about a slot that exists and a slot that cannot, and see which
/// byte differs.
#[test]
fn find_existing_save_layout() {
    let Some(api) = api_or_skip() else { return };
    let selector = game_instance_selector(&api);

    let name = api.op(
        "call",
        json!({
            "class": GAME_INSTANCE,
            "function": "SGK GetSaveGameSlotName",
            "instance_selector": selector,
            "parms_hex": "0".repeat(32),
        }),
    );
    assert!(name.ok, "SGK GetSaveGameSlotName failed: {:?}", name.error);
    let slot_fstring = name.result["parms_hex_after"].as_str().unwrap_or("").to_string();
    println!("slot {:?} -> FString {slot_fstring}", read_fstring(&api, &slot_fstring));

    let hosts = modforge::client::walk_class_chain_instances(&api, "BP_HostNewGameServer_C", 8);
    let host = hosts
        .iter()
        .find(|h| {
            h.full_name.contains("BP_HostLoadGameServer")
                && h.full_name.contains("/Engine/Transient")
        })
        .expect("no live BP_HostLoadGameServer");

    // Real slot: the FString the game just handed us.
    let real = format!("{slot_fstring}00");
    // Impossible slot: a null FString, which no save can be
    // stored under.
    let empty = "0".repeat(34);

    for (label, parms) in [("existing slot", real), ("empty slot", empty)] {
        let r = api.op(
            "call",
            json!({
                "class": "BP_HostNewGameServer_C",
                "function": "FindExistingSave",
                "instance_selector": host.addr_selector,
                "parms_hex": parms,
            }),
        );
        if !r.ok {
            println!("{label}: FAILED {:?}", r.error);
            continue;
        }
        println!("{label}: {}", r.result["parms_hex_after"].as_str().unwrap_or(""));
    }
}

/// Load the save the game instance already points at.
///
/// Three steps, all of them the game's own functions:
///   1. `SGK SetLoadSaveGame(true)`  - load, do not start new
///   2. verify the flag took
///   3. `LoadLevel()` on BP_HostLoadGameServer
///
/// No FString has to be built: `read_current_slot` shows the
/// instance already holds "Save 1".
///
/// CAUTION: `LoadLevel` is also the New Game path. The flag in
/// step 1 is what makes it load instead of overwrite, so step 2
/// must pass before step 3 runs.
#[test]
#[ignore = "starts a level load; run deliberately"]
fn load_current_slot() {
    let Some(api) = api_or_skip() else { return };
    let selector = game_instance_selector(&api);

    let name = api.op(
        "call",
        json!({
            "class": GAME_INSTANCE,
            "function": "SGK GetSaveGameSlotName",
            "instance_selector": selector,
            "parms_hex": "0".repeat(32),
        }),
    );
    assert!(name.ok, "SGK GetSaveGameSlotName failed: {:?}", name.error);
    let slot = read_fstring(&api, name.result["parms_hex_after"].as_str().unwrap_or(""));
    println!("slot to load: {slot:?}");
    assert!(!slot.is_empty(), "no slot name set; refusing to load");

    let set = api.op(
        "call",
        json!({
            "class": GAME_INSTANCE,
            "function": "SGK SetLoadSaveGame",
            "instance_selector": selector,
            "parms_hex": "01",
        }),
    );
    assert!(set.ok, "SGK SetLoadSaveGame failed: {:?}", set.error);

    let check = api.op(
        "call",
        json!({
            "class": GAME_INSTANCE,
            "function": "SGK GetLoadSaveGame",
            "instance_selector": selector,
            "parms_hex": "00",
        }),
    );
    assert!(check.ok, "SGK GetLoadSaveGame failed: {:?}", check.error);
    assert_eq!(
        check.result["parms_hex_after"].as_str(),
        Some("01"),
        "load flag did not take; refusing to call LoadLevel"
    );
    println!("load flag set");

    // The load instance, not the new-game one. Both are
    // BP_HostNewGameServer_C; only the name distinguishes them.
    let hosts = modforge::client::walk_class_chain_instances(&api, "BP_HostNewGameServer_C", 8);
    for h in &hosts {
        println!("  host: {}", h.full_name);
    }
    // /Game/... entries are the class template inside the
    // WidgetTree package; only /Engine/Transient is the widget
    // actually on screen. Calling the template does nothing.
    let host = hosts
        .iter()
        .find(|h| {
            h.full_name.contains("BP_HostLoadGameServer")
                && h.full_name.contains("/Engine/Transient")
        })
        .expect("no live BP_HostLoadGameServer");
    println!("calling LoadLevel on {}", host.full_name);

    let go = api.op(
        "call",
        json!({
            "class": "BP_HostNewGameServer_C",
            "function": "LoadLevel",
            "instance_selector": host.addr_selector,
            "parms_hex": "",
        }),
    );
    println!("LoadLevel -> ok={} {:?}", go.ok, go.result);
    assert!(go.ok, "LoadLevel failed: {:?}", go.error);
}

/// Everything live under the main menu, so nothing is missed if
/// the load path lives on a class not listed above.
#[test]
fn menu_widget_tree() {
    let Some(api) = api_or_skip() else { return };
    let r = api.op("walk_class_chain", json!({"needle": "UserWidget", "max": 400}));
    assert!(r.ok, "walk failed: {:?}", r.error);
    let all = r.result["instances"].as_array().cloned().unwrap_or_default();
    let mut live: Vec<&str> = all
        .iter()
        .filter_map(|w| w["full_name"].as_str())
        .filter(|n| n.contains("/Engine/Transient"))
        .collect();
    live.sort_unstable();
    println!("{} live widget(s):", live.len());
    for n in live {
        println!("  {n}");
    }
}
