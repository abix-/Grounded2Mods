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
