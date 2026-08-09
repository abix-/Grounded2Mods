//! Save/reload survival test for custom NPCs.
//!
//! Spawns custom goons, records their positions and indices,
//! then prints instructions for the operator to save/reload.
//! After reload, checks if the NPCs still exist.
//!
//! Run BEFORE save: cargo test ... spawn_before_save
//! Operator saves and reloads the game.
//! Run AFTER reload: cargo test ... check_after_reload
//!
//! ```text
//! cargo test -p schedule1-mod --test research_save_reload. --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, ping_or_skip, player_position};
use serde_json::json;

const FACTORY: &str = "Unityforge.Shim.Schedule1.NpcFactory";

fn factory_call(
    api: &modforge::client::Api<serde_json::Value>,
    method: &str,
    args: serde_json::Value,
) -> Option<serde_json::Value> {
    let r = api.op("invoke_static", json!({"class": FACTORY, "method": method, "args": args}));
    if !r.ok {
        println!("{method}: op failed: {:?}", r.error);
        return None;
    }
    let s = match r.result.as_str() {
        Some(s) => s,
        None => {
            println!("{method}: result not a string: {}", r.result);
            return None;
        }
    };
    let parsed: serde_json::Value = match serde_json::from_str(s) {
        Ok(v) => v,
        Err(e) => {
            println!("{method}: bad json: {e}: {s}");
            return None;
        }
    };
    if parsed["ok"].as_bool() != Some(true) {
        println!("{method}: not ok: {}", parsed["error"]);
        return None;
    }
    Some(parsed)
}

#[test]
fn spawn_before_save() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position; load a save first");
        return;
    };

    // spawn 3 goons in a line
    let positions = [
        (px + 5.0, py, pz + 2.0),
        (px + 8.0, py, pz + 2.0),
        (px + 11.0, py, pz + 2.0),
    ];

    let mut indices = Vec::new();
    for (i, (x, y, z)) in positions.iter().enumerate() {
        let spawn = factory_call(&api, "SpawnGoon", json!([x, y, z]));
        match &spawn {
            Some(sv) => {
                let idx = sv["index"].as_i64().unwrap_or(0);
                println!("goon {} spawned index={idx} at ({:.1}, {:.1}, {:.1})", i, x, y, z);
                indices.push(idx);
            }
            None => println!("goon {} SpawnGoon failed", i),
        }
    }

    println!("\nwaiting 10s for spawn pipeline...");
    std::thread::sleep(std::time::Duration::from_secs(10));

    // enable idle on all
    for &idx in &indices {
        let idle = factory_call(&api, "EnableIdleBehaviour", json!([idx]));
        match &idle {
            Some(v) => println!("goon {idx} idle enabled: active={}", v["active_type"]),
            None => println!("goon {idx} EnableIdleBehaviour failed"),
        }
    }

    // get custom NPC count
    let count = factory_call(&api, "CustomNpcCount", json!([]));
    match &count {
        Some(v) => println!("\ncustom NPC count: {}", v["count"]),
        None => println!("CustomNpcCount failed"),
    }

    println!("\n=== INSTRUCTIONS ===");
    println!("spawned indices: {:?}", indices);
    println!("1. SAVE the game now (quicksave or menu save)");
    println!("2. RELOAD the save (load from menu or quickload)");
    println!("3. Run: cargo test -p schedule1-mod --test research_save_reload -- --test-threads=1 --nocapture check_after_reload");
}

#[test]
fn check_after_reload() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // check custom NPC count
    let count = factory_call(&api, "CustomNpcCount", json!([]));
    match &count {
        Some(v) => {
            let c = v["count"].as_i64().unwrap_or(0);
            let ready = v["ready"].as_bool().unwrap_or(false);
            println!("custom NPC count after reload: {c}, ready: {ready}");
            if c == 0 {
                println!("FACT: custom NPCs do NOT survive save/reload (count=0)");
                println!("the mod must respawn NPCs on load");
                return;
            }
        }
        None => println!("CustomNpcCount failed"),
    }

    // try to query indices 0-10 to see if any survived
    println!("\nchecking indices 0-10...");
    let mut found = 0;
    for idx in 0..11 {
        let state = factory_call(&api, "GetBehaviourState", json!([idx]));
        match &state {
            Some(v) => {
                let active = v["active_type"].as_str().unwrap_or("null");
                println!("index {idx}: active={active} (ALIVE)");
                found += 1;
            }
            None => {}
        }
    }

    println!("\n=== VERDICT ===");
    if found > 0 {
        println!("FACT: {found} custom NPCs survived save/reload");
    } else {
        println!("FACT: NO custom NPCs survived save/reload");
        println!("the mod must respawn NPCs on load");
    }
    println!("OPERATOR CHECK: are the goons still visible in-game?");
}
