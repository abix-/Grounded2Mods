//! Movement speed research: can we change a custom NPC's speed?
//!
//! Spawns a goon, reads default speed multiplier, sets it to 2x,
//! reads again to confirm.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_speed. --test-threads=1 --nocapture
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
fn speed_multiplier_write() {
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

    // read default speed
    let speed1 = factory_call(&api, "SetSpeedMultiplier", json!([idx, 1.0]));
    match &speed1 {
        Some(v) => println!("default speed: before={} after={}", v["before"], v["after"]),
        None => {
            println!("SetSpeedMultiplier failed (method may not exist)");
            return;
        }
    }

    // set 2x speed
    let speed2 = factory_call(&api, "SetSpeedMultiplier", json!([idx, 2.0]));
    match &speed2 {
        Some(v) => {
            let before = v["before"].as_f64().unwrap_or(0.0);
            let after = v["after"].as_f64().unwrap_or(0.0);
            println!("set 2x: before={before} after={after}");
            if (after - 2.0).abs() < 0.01 {
                println!("FACT: SpeedMultiplier write works on custom NPCs");
            } else {
                println!("speed did NOT change to 2.0 (got {after})");
            }
        }
        None => println!("SetSpeedMultiplier(2.0) failed"),
    }

    // set 0.5x speed
    let speed3 = factory_call(&api, "SetSpeedMultiplier", json!([idx, 0.5]));
    match &speed3 {
        Some(v) => {
            let after = v["after"].as_f64().unwrap_or(0.0);
            println!("set 0.5x: after={after}");
        }
        None => println!("SetSpeedMultiplier(0.5) failed"),
    }

    println!("\n=== VERDICT ===");
    println!("OPERATOR CHECK: does the goon visually move faster/slower?");
}
