//! Test whether RetaliateAgainstPlayer makes a custom goon fight
//! the player. Spawn, idle hold, trigger retaliation, observe.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_retaliation. --test-threads=1 --nocapture
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
fn retaliate_against_player() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position");
        return;
    };

    // Spawn goon near player
    let Some(spawn) = factory_call(&api, "SpawnGoon", json!([px + 3.0, py, pz + 3.0])) else {
        println!("SpawnGoon failed");
        return;
    };
    let idx = spawn["index"].as_i64().unwrap_or(0);
    println!("spawned goon index={idx}, waiting 10s for settle...");
    std::thread::sleep(std::time::Duration::from_secs(10));

    // Enable idle so it holds position
    println!("\n=== EnableIdleBehaviour ===");
    let _idle = factory_call(&api, "EnableIdleBehaviour", json!([idx]));

    // Check state before
    println!("\n=== Before retaliation ===");
    if let Some(v) = factory_call(&api, "GetBehaviourState", json!([idx])) {
        println!(
            "  active: {}, enabled: {}",
            v["active_type"], v["enabled_count"]
        );
    }

    // Trigger retaliation
    println!("\n=== RetaliateAgainstPlayer ===");
    let ret = factory_call(&api, "RetaliateAgainstPlayer", json!([idx]));
    println!("  result: {:?}", ret);

    // Watch for 30s
    println!("\n=== Watching behaviour for 30s ===");
    for tick in 0..6 {
        std::thread::sleep(std::time::Duration::from_secs(5));
        if let Some(v) = factory_call(&api, "GetBehaviourState", json!([idx])) {
            let active = v["active_type"].as_str().unwrap_or("null");
            let enabled = v["enabled_count"].as_i64().unwrap_or(0);
            println!("  tick {tick}: active={active} enabled={enabled}");
        } else {
            println!("  tick {tick}: GetBehaviourState failed");
            break;
        }
    }
}
