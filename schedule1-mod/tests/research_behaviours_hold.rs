//! Phase 2 refined: hold-post via IdleBehaviour with NO
//! IdlePoint. Spawns a goon, enables idle immediately, waits
//! 30s, checks position drift.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_behaviours_hold -- --test-threads=1 --nocapture
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

fn pos_from(v: &serde_json::Value) -> (f64, f64, f64) {
    (
        v["pos_x"].as_f64().unwrap_or(0.0),
        v["pos_y"].as_f64().unwrap_or(0.0),
        v["pos_z"].as_f64().unwrap_or(0.0),
    )
}

fn dist(a: (f64, f64, f64), b: (f64, f64, f64)) -> f64 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    let dz = a.2 - b.2;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[test]
fn idle_no_point_holds_position() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position; load a save first");
        return;
    };

    let spawn_pos = (px + 8.0, py, pz + 3.0);
    let spawn = factory_call(
        &api,
        "SpawnGoon",
        json!([spawn_pos.0, spawn_pos.1, spawn_pos.2]),
    );
    let Some(ref sv) = spawn else {
        println!("SpawnGoon failed");
        return;
    };
    let idx = sv["index"].as_i64().unwrap_or(0);
    println!("spawned goon index={idx} at ({:.1}, {:.1}, {:.1})", spawn_pos.0, spawn_pos.1, spawn_pos.2);

    println!("waiting 10s for spawn pipeline...");
    std::thread::sleep(std::time::Duration::from_secs(10));

    // enable idle (no idle point)
    let enable = factory_call(&api, "EnableIdleBehaviour", json!([idx]));
    let Some(ref ev) = enable else {
        println!("EnableIdleBehaviour failed");
        return;
    };
    let initial_pos = pos_from(ev);
    println!(
        "idle enabled at ({:.2}, {:.2}, {:.2}), active={}",
        initial_pos.0, initial_pos.1, initial_pos.2,
        ev["active_type"]
    );

    // check position at 10s intervals
    for wait in [10, 10, 10] {
        println!("waiting {wait}s...");
        std::thread::sleep(std::time::Duration::from_secs(wait));
        let state = factory_call(&api, "GetBehaviourState", json!([idx]));
        let Some(ref sv) = state else {
            println!("GetBehaviourState failed");
            return;
        };
        let cur = pos_from(sv);
        let drift = dist(initial_pos, cur);
        println!(
            "pos=({:.2}, {:.2}, {:.2}) drift={:.2}m active={}",
            cur.0, cur.1, cur.2, drift, sv["active_type"]
        );
    }

    // final verdict
    let final_state = factory_call(&api, "GetBehaviourState", json!([idx]));
    let Some(ref fv) = final_state else {
        println!("final GetBehaviourState failed");
        return;
    };
    let final_pos = pos_from(fv);
    let total_drift = dist(initial_pos, final_pos);
    println!("\n=== VERDICT ===");
    println!("total drift after 30s: {:.2}m", total_drift);
    if total_drift < 1.0 {
        println!("FACT: IdleBehaviour with no IdlePoint holds position (drift < 1m)");
    } else {
        println!("NPC drifted {:.2}m (NOT holding)", total_drift);
    }
}

#[test]
fn idle_with_point_wanders() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position; load a save first");
        return;
    };

    let spawn = factory_call(&api, "SpawnGoon", json!([px + 5.0, py, pz - 5.0]));
    let Some(ref sv) = spawn else {
        println!("SpawnGoon failed");
        return;
    };
    let idx = sv["index"].as_i64().unwrap_or(0);

    println!("waiting 6s for spawn pipeline...");
    std::thread::sleep(std::time::Duration::from_secs(6));

    // set idle point 10m away and enable
    let target = (px + 15.0, py, pz - 5.0);
    let set = factory_call(
        &api,
        "SetIdlePoint",
        json!([idx, target.0, target.1, target.2]),
    );
    let Some(ref setv) = set else {
        println!("SetIdlePoint failed");
        return;
    };
    let initial_pos = pos_from(setv);
    println!(
        "idle point set to ({:.1}, {:.1}, {:.1}), NPC at ({:.2}, {:.2}, {:.2})",
        target.0, target.1, target.2,
        initial_pos.0, initial_pos.1, initial_pos.2
    );

    // check at intervals
    for wait in [10, 10, 10] {
        println!("waiting {wait}s...");
        std::thread::sleep(std::time::Duration::from_secs(wait));
        let state = factory_call(&api, "GetBehaviourState", json!([idx]));
        let Some(ref sv) = state else {
            println!("GetBehaviourState failed");
            return;
        };
        let cur = pos_from(sv);
        let to_target = dist(cur, target);
        let from_spawn = dist(cur, initial_pos);
        println!(
            "pos=({:.2}, {:.2}, {:.2}) dist_to_target={:.1}m dist_from_spawn={:.1}m",
            cur.0, cur.1, cur.2, to_target, from_spawn
        );
    }
    println!("OPERATOR CHECK: did the goon walk toward the target, past it, or wander randomly?");
}
