//! Walk the game's Mono type system: list singletons, walk classes,
//! and inspect objects to find the entry points for game state.
//!
//! ```text
//! FISH_DEBUG_PORT=17174 cargo test -p fish-mod --test research_classes -- --ignored --test-threads=1 --nocapture
//! ```

mod common;
use common::api_or_skip;
use serde_json::json;

#[test]
#[ignore]
fn list_singletons() {
    let Some(api) = api_or_skip() else { return };
    let r = api.op("list_singletons", json!({}));
    assert!(r.ok, "list_singletons failed: {:?}", r.error);
    let singletons = r.result["singletons"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    println!("{} singleton(s):", singletons.len());
    for s in &singletons {
        println!("  {}", serde_json::to_string(s).unwrap_or_default());
    }
}

#[test]
#[ignore]
fn walk_game_manager() {
    let Some(api) = api_or_skip() else { return };
    for class in [
        "GameManager",
        "FishingManager",
        "PlayerController",
        "PlayerManager",
        "NetworkManager",
        "UIManager",
        "InventoryManager",
        "ShopManager",
        "WeatherManager",
        "TimeManager",
    ] {
        let r = api.op("walk_class", json!({"class": class, "max": 10}));
        if !r.ok {
            println!("{class}: not found");
            continue;
        }
        let instances = r.result["instances"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        println!("{class}: {} instance(s)", instances.len());
        for inst in &instances {
            let handle = inst["handle"].as_i64().unwrap_or(-1);
            println!("  handle={handle}");
            if handle >= 0 {
                let inspect = api.op("inspect_object", json!({"handle": handle}));
                if inspect.ok {
                    println!(
                        "  fields: {}",
                        serde_json::to_string_pretty(&inspect.result).unwrap_or_default()
                    );
                }
            }
        }
    }
}

#[test]
#[ignore]
fn walk_all_loaded_classes() {
    let Some(api) = api_or_skip() else { return };
    let r = api.op("list_classes", json!({}));
    assert!(r.ok, "list_classes failed: {:?}", r.error);
    let classes = r.result["classes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    println!("{} class(es) loaded:", classes.len());
    for c in &classes {
        let name = c["name"].as_str().unwrap_or("?");
        let ns = c["namespace"].as_str().unwrap_or("");
        if !ns.is_empty() {
            println!("  {ns}.{name}");
        } else {
            println!("  {name}");
        }
    }
}
