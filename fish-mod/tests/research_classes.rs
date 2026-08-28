//! Walk the game's Mono type system: probe candidate class names,
//! inspect key objects, and find the entry points for game state.
//!
//! ```text
//! FISH_DEBUG_PORT=17174 cargo test -p fish-mod --test research_classes -- --ignored --test-threads=1 --nocapture
//! ```

mod common;
use common::api_or_skip;
use serde_json::json;

const CANDIDATE_CLASSES: &[&str] = &[
    "SceneManager",
    "AudioManager",
    "SpawnManager",
    "IslandManager",
    "PlayerHolder",
    "LocalPlayer",
    "Boat",
    "BoatInteractable",
    "VisualPhysicsBoat",
    "Seagull",
    "Clam",
    "ClamSpawner",
    "SeagullSpawner",
    "SlotMachine",
    "EndGameToggles",
    "Knife",
    "ItemDot",
    "HitMarkerUI",
    "CanvasText",
    "CharacterInteractable",
    "LocalUI",
    "InventorySlot",
    "Chat",
    "PlayerController",
    "Player",
    "FishingRod",
    "Fish",
    "FishSpawner",
    "Inventory",
    "Item",
    "Shop",
    "Weather",
    "DayNightCycle",
    "Quest",
    "QuestManager",
    "SaveManager",
    "LobbyManager",
    "ServerManager",
    "ClientManager",
];

#[test]
#[ignore]
fn probe_all_candidates() {
    let Some(api) = api_or_skip() else { return };
    println!(
        "probing {} candidate class names...\n",
        CANDIDATE_CLASSES.len()
    );
    let mut found = Vec::new();
    for class in CANDIDATE_CLASSES {
        let r = api.op("walk_class", json!({"class": class, "max": 5}));
        if !r.ok {
            continue;
        }
        let instances = r.result["instances"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if instances.is_empty() {
            continue;
        }
        println!("{class}: {} instance(s)", instances.len());
        found.push(*class);
        let first = &instances[0];
        let handle = first["handle"].as_i64().unwrap_or(-1);
        if handle >= 0 {
            let inspect = api.op("inspect_object", json!({"handle": handle}));
            if inspect.ok {
                println!(
                    "  fields: {}",
                    serde_json::to_string_pretty(&inspect.result).unwrap_or_default()
                );
            }
            api.op("release_handle", json!({"handle": handle}));
        }
        println!();
    }
    println!(
        "--- found {} of {} candidates",
        found.len(),
        CANDIDATE_CLASSES.len()
    );
    for c in &found {
        println!("  {c}");
    }
}

#[test]
#[ignore]
fn list_singletons() {
    let Some(api) = api_or_skip() else { return };
    let types: Vec<_> = CANDIDATE_CLASSES.iter().map(|s| json!(s)).collect();
    let r = api.op("list_singletons", json!({"types": types}));
    assert!(r.ok, "list_singletons failed: {:?}", r.error);
    let singletons = r.result["singletons"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    println!("{} singleton probe(s):", singletons.len());
    for s in &singletons {
        let class = s["class"].as_str().unwrap_or("?");
        let found = s["found"].as_bool().unwrap_or(false);
        if found {
            println!("  {class}: FOUND (handle={})", s["handle"]);
        }
    }
}

#[test]
#[ignore]
fn inspect_scene_manager() {
    let Some(api) = api_or_skip() else { return };
    let r = api.op("walk_class", json!({"class": "SceneManager", "max": 3}));
    assert!(r.ok, "walk_class failed: {:?}", r.error);
    let instances = r.result["instances"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    println!("SceneManager: {} instance(s)", instances.len());
    for inst in &instances {
        let handle = inst["handle"].as_i64().unwrap_or(-1);
        if handle < 0 {
            continue;
        }
        let inspect = api.op("inspect_object", json!({"handle": handle}));
        if inspect.ok {
            println!(
                "{}",
                serde_json::to_string_pretty(&inspect.result).unwrap_or_default()
            );
        }
        modforge::client::print_declared_methods(&api, "SceneManager");
    }
}

#[test]
#[ignore]
fn inspect_player() {
    let Some(api) = api_or_skip() else { return };
    for class in ["PlayerHolder", "LocalPlayer", "Player", "PlayerController"] {
        let r = api.op("walk_class", json!({"class": class, "max": 3}));
        if !r.ok {
            continue;
        }
        let instances = r.result["instances"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if instances.is_empty() {
            continue;
        }
        println!("{class}: {} instance(s)", instances.len());
        let handle = instances[0]["handle"].as_i64().unwrap_or(-1);
        if handle < 0 {
            continue;
        }
        let inspect = api.op("inspect_object", json!({"handle": handle}));
        if inspect.ok {
            println!(
                "{}",
                serde_json::to_string_pretty(&inspect.result).unwrap_or_default()
            );
        }
        modforge::client::print_declared_methods(&api, class);
        println!();
    }
}
