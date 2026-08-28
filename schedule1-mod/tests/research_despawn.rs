//! Despawn and cleanup research on custom NPCs.
//!
//! Tests:
//! 1. KillNpc: programmatic kill via S1API Kill() (TakeDamage to
//!    max). Does the NPC die? Does the Die hook fire? Does the
//!    body get cleaned up?
//! 2. DespawnNpc: FishNet ServerManager.Despawn. Does the NPC
//!    vanish? Does GetBehaviourState fail after?
//!
//! Requires the updated shim with KillNpc + DespawnNpc methods.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_despawn. --test-threads=1 --nocapture
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
    let r = api.op(
        "invoke_static",
        json!({"class": FACTORY, "method": method, "args": args}),
    );
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
fn kill_npc_programmatic() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position; load a save first");
        return;
    };

    let spawn = factory_call(&api, "SpawnGoon", json!([px + 5.0, py, pz + 3.0]));
    let Some(ref sv) = spawn else {
        println!("SpawnGoon failed");
        return;
    };
    let idx = sv["index"].as_i64().unwrap_or(0);
    println!("spawned goon index={idx}");

    println!("waiting 10s for spawn pipeline...");
    std::thread::sleep(std::time::Duration::from_secs(10));

    // check state before kill
    let before = factory_call(&api, "GetBehaviourState", json!([idx]));
    match &before {
        Some(v) => println!(
            "before kill: active={} enabled={}",
            v["active_type"], v["enabled_count"]
        ),
        None => println!("GetBehaviourState failed before kill"),
    }

    // kill it
    println!("calling KillNpc({idx})...");
    let kill = factory_call(&api, "KillNpc", json!([idx]));
    match &kill {
        Some(_) => println!("KillNpc returned ok"),
        None => {
            println!("KillNpc failed (method may not exist in deployed shim)");
            return;
        }
    }

    // check state after kill
    std::thread::sleep(std::time::Duration::from_secs(2));
    let after = factory_call(&api, "GetBehaviourState", json!([idx]));
    match &after {
        Some(v) => {
            let active = v["active_type"].as_str().unwrap_or("null");
            println!("after kill: active={active} enabled={}", v["enabled_count"]);
            if active.contains("Dead") {
                println!("FACT: KillNpc transitions the NPC to DeadBehaviour");
            }
        }
        None => println!("GetBehaviourState failed after kill (NPC may be destroyed)"),
    }

    // check body cleanup after 30s
    println!("waiting 30s to check body cleanup...");
    std::thread::sleep(std::time::Duration::from_secs(30));
    let cleanup = factory_call(&api, "GetBehaviourState", json!([idx]));
    match &cleanup {
        Some(v) => {
            let active = v["active_type"].as_str().unwrap_or("null");
            println!("30s after kill: active={active}");
            println!("body is STILL present in the world after 30s");
        }
        None => println!("30s after kill: GetBehaviourState failed (body cleaned up)"),
    }

    println!("\n=== VERDICT ===");
    println!("OPERATOR CHECK: MelonLoader log for 'npc down' with this goon's ptr");
    println!("OPERATOR CHECK: is the body visible in-game?");
}

#[test]
fn despawn_npc_clean() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position; load a save first");
        return;
    };

    let spawn = factory_call(&api, "SpawnGoon", json!([px + 8.0, py, pz + 3.0]));
    let Some(ref sv) = spawn else {
        println!("SpawnGoon failed");
        return;
    };
    let idx = sv["index"].as_i64().unwrap_or(0);
    println!("spawned goon index={idx}");

    println!("waiting 10s for spawn pipeline...");
    std::thread::sleep(std::time::Duration::from_secs(10));

    // verify NPC exists
    let before = factory_call(&api, "GetBehaviourState", json!([idx]));
    match &before {
        Some(v) => println!(
            "before despawn: active={} enabled={}",
            v["active_type"], v["enabled_count"]
        ),
        None => {
            println!("GetBehaviourState failed before despawn (NPC never fully spawned)");
            return;
        }
    }

    // despawn
    println!("calling DespawnNpc({idx})...");
    let desp = factory_call(&api, "DespawnNpc", json!([idx]));
    match &desp {
        Some(_) => println!("DespawnNpc returned ok"),
        None => {
            println!("DespawnNpc failed (method may not exist in deployed shim)");
            return;
        }
    }

    // check state after despawn
    std::thread::sleep(std::time::Duration::from_secs(2));
    let after = factory_call(&api, "GetBehaviourState", json!([idx]));
    match &after {
        Some(v) => {
            let active = v["active_type"].as_str().unwrap_or("null");
            println!("after despawn: active={active} (NPC still accessible)");
        }
        None => println!("after despawn: GetBehaviourState failed (NPC removed from world)"),
    }

    println!("\n=== VERDICT ===");
    println!("OPERATOR CHECK: is the NPC visually gone from the game world?");
    println!("OPERATOR CHECK: any errors in MelonLoader log?");
}
