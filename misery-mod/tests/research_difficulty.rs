//! Where do the per-preset gameplay settings come from?
//!
//! `ShiningsTimer` reads 22 on `DifficultyPreset` 4, live in the
//! GameInstance. Two things are unknown and both matter before
//! writing anything: what the other presets hold, and who writes
//! the struct. If the game re-applies a preset on a new
//! expedition or a save load, a write into the live struct gets
//! stomped.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_difficulty -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live, show};
use serde_json::json;

/// Every data table the game has loaded, so we can see whether
/// the presets live in one.
#[test]
fn data_tables() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let r = api.op("discover_data_tables", json!({}));
    if !r.ok {
        println!("discover_data_tables failed: {:?}", r.error);
        return;
    }
    // Print names only; the full schema dump is enormous.
    match r.result.as_array() {
        Some(list) => {
            println!("{} data table(s)", list.len());
            for t in list {
                println!("  {}", t["name"].as_str().unwrap_or(&t.to_string()));
            }
        }
        None => println!("{}", r.result),
    }
}

/// The difficulty enum's entries, and anything named like a
/// preset source.
#[test]
fn difficulty_surface() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }

    for name in ["E_Difficulty", "S_GameplaySettings"] {
        println!("=== struct/enum detail: {name} ===");
        let r = api.op("discover_struct_detail", json!({"name": name}));
        show(name, &r);
    }
}

/// Who touches GameplaySettings? The GameInstance declares
/// `GetGameplaySettings`; this dumps its full class detail so we
/// can see every function that could write the struct.
#[test]
fn game_instance_detail() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let r = api.op("discover_class_detail", json!({"name": "BP_SGKGameInstance_C"}));
    show("BP_SGKGameInstance_C", &r);
}

/// `DifficultyList` is the per-preset source of the gameplay
/// settings. Every row is one difficulty; the `ShiningsTimer`
/// column is the interval the live GameInstance struct gets
/// filled from.
#[test]
fn dump_difficulty_list() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let r = api.op("dump_data_table", json!({"table_name": "DifficultyList"}));
    show("DifficultyList", &r);
}
