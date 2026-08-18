//! Phase 1: behaviour state and idle enable.
//!
//! Spawns a goon, reads its full behaviour state, then enables
//! IdleBehaviour and reads again. Proves whether
//! EnableIdleBehaviour makes idle the activeBehaviour.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_behaviours_phase1 --test-threads=1 --nocapture
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
    let s = r.result.as_str()?;
    let parsed: serde_json::Value = serde_json::from_str(s).ok()?;
    if parsed["ok"].as_bool() != Some(true) {
        println!("{method}: not ok: {}", parsed["error"]);
        return None;
    }
    Some(parsed)
}

#[test]
fn behaviour_state_and_idle_enable() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position; load a save first");
        return;
    };

    // spawn a goon near the player
    let spawn = factory_call(&api, "SpawnGoon", json!([px + 5.0, py, pz]));
    let Some(ref sv) = spawn else {
        println!("SpawnGoon failed");
        return;
    };
    let idx = sv["index"].as_i64().unwrap_or(0);
    println!("spawned goon index={idx}");

    // wait for spawn pipeline
    println!("waiting 8s for spawn to settle...");
    std::thread::sleep(std::time::Duration::from_secs(8));

    // read behaviour state before any changes
    println!("\n=== BEFORE EnableIdleBehaviour ===");
    let before = factory_call(&api, "GetBehaviourState", json!([idx]));
    match &before {
        Some(v) => println!("{}", serde_json::to_string_pretty(v).unwrap_or_default()),
        None => {
            println!("GetBehaviourState failed");
            return;
        }
    }

    // enable idle behaviour
    println!("\n=== EnableIdleBehaviour ===");
    let after = factory_call(&api, "EnableIdleBehaviour", json!([idx]));
    match &after {
        Some(v) => println!("{}", serde_json::to_string_pretty(v).unwrap_or_default()),
        None => {
            println!("EnableIdleBehaviour failed");
            return;
        }
    }

    // compare key fields
    let b = before.unwrap();
    let a = after.unwrap();
    println!("\n=== COMPARISON ===");
    println!(
        "active_type: {} -> {}",
        b["active_type"], a["active_type"]
    );
    println!(
        "idle_enabled: {} -> {}",
        b["idle_enabled"], a["idle_enabled"]
    );
    println!(
        "idle_active: {} -> {}",
        b["idle_active"], a["idle_active"]
    );
    println!(
        "enabled_count: {} -> {}",
        b["enabled_count"], a["enabled_count"]
    );
    println!("OPERATOR CHECK: is idle now the activeBehaviour?");
}

#[test]
fn set_idle_point_holds_position() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position; load a save first");
        return;
    };

    // spawn a goon
    let spawn = factory_call(&api, "SpawnGoon", json!([px + 5.0, py, pz + 5.0]));
    let Some(ref sv) = spawn else {
        println!("SpawnGoon failed");
        return;
    };
    let idx = sv["index"].as_i64().unwrap_or(0);
    println!("spawned goon index={idx}");

    println!("waiting 8s for spawn to settle...");
    std::thread::sleep(std::time::Duration::from_secs(8));

    // read initial state and position
    println!("\n=== BEFORE SetIdlePoint ===");
    let before = factory_call(&api, "GetBehaviourState", json!([idx]));
    match &before {
        Some(v) => {
            println!("pos: ({}, {}, {})", v["pos_x"], v["pos_y"], v["pos_z"]);
            println!("idle_point_set: {}", v["idle_point_set"]);
            println!("active_type: {}", v["active_type"]);
        }
        None => {
            println!("GetBehaviourState failed");
            return;
        }
    }

    // set idle point 15m away from spawn
    let hold_x = px + 20.0;
    let hold_z = pz + 5.0;
    println!(
        "\n=== SetIdlePoint to ({}, {}, {}) ===",
        hold_x, py, hold_z
    );
    let set = factory_call(&api, "SetIdlePoint", json!([idx, hold_x, py, hold_z]));
    match &set {
        Some(v) => {
            println!("idle_point_set: {}", v["idle_point_set"]);
            println!("idle_enabled: {}", v["idle_enabled"]);
            println!("active_type: {}", v["active_type"]);
        }
        None => {
            println!("SetIdlePoint failed");
            return;
        }
    }

    // wait and check if NPC moved toward the idle point
    println!("\nwaiting 15s for NPC to walk to idle point...");
    std::thread::sleep(std::time::Duration::from_secs(15));

    println!("\n=== AFTER 15s ===");
    let after = factory_call(&api, "GetBehaviourState", json!([idx]));
    match &after {
        Some(v) => {
            println!("pos: ({}, {}, {})", v["pos_x"], v["pos_y"], v["pos_z"]);
            println!("active_type: {}", v["active_type"]);
            let dx = v["pos_x"].as_f64().unwrap_or(0.0) - hold_x;
            let dz = v["pos_z"].as_f64().unwrap_or(0.0) - hold_z;
            let dist = (dx * dx + dz * dz).sqrt();
            println!("distance from idle point: {:.1}m", dist);
            if dist < 3.0 {
                println!("FACT: NPC walked to idle point and is holding");
            } else {
                println!("NPC did NOT reach idle point (dist={:.1}m)", dist);
            }
        }
        None => println!("GetBehaviourState failed after wait"),
    }
    println!("OPERATOR CHECK: did the goon walk to and stay at the idle point?");
}
