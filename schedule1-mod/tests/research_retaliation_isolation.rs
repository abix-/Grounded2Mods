//! Isolate what makes custom goons fight back when punched.
//! Two goons, each with one variable changed:
//!   A: base NPCResponses (swap from civilian), Aggression 0.1 (default)
//!   B: NPCResponses_Civilian (default), Aggression 1.0
//!
//! Operator punches each goon and we watch behaviour state.
//! If A retaliates, Responses type is the key.
//! If B retaliates, Aggression is the key.
//! If both or neither, it is a combination.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_retaliation_isolation -- --test-threads=1 --nocapture
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

fn spawn_and_configure(
    api: &modforge::client::Api<serde_json::Value>,
    label: &str,
    x: f64,
    y: f64,
    z: f64,
    swap_responses: bool,
    aggression: Option<f32>,
) -> Option<i64> {
    println!("\n=== Spawning {label} at ({x:.0}, {y:.0}, {z:.0}) ===");
    let spawn = factory_call(api, "SpawnGoon", json!([x, y, z]))?;
    let idx = spawn["index"].as_i64().unwrap_or(0);
    println!("  index={idx}, waiting 10s...");
    std::thread::sleep(std::time::Duration::from_secs(10));

    // Enable idle to hold position
    factory_call(api, "EnableIdleBehaviour", json!([idx]));

    if swap_responses {
        println!("  swapping Responses to base NPCResponses...");
        let r = factory_call(api, "SetResponsesBase", json!([idx]));
        println!("  SetResponsesBase: {:?}", r);
    }

    if let Some(agg) = aggression {
        println!("  setting Aggression to {agg}...");
        let r = factory_call(api, "SetAggression", json!([idx, agg]));
        println!("  SetAggression: {:?}", r);
    }

    // Verify config
    println!("  verifying config...");
    let cfg = factory_call(api, "InspectCombatConfig", json!([idx]));
    if let Some(c) = cfg {
        println!("  responses_type: {}", c["responses_type"]);
        println!("  aggression: {}", c["aggression"]);
        println!(
            "  awareness_responses_type: {}",
            c["awareness_responses_type"]
        );
    }

    Some(idx)
}

fn watch_behaviour(
    api: &modforge::client::Api<serde_json::Value>,
    label: &str,
    idx: i64,
    seconds: u64,
    interval: u64,
) {
    println!("\n=== Watching {label} (index {idx}) for {seconds}s ===");
    println!("  >>> PUNCH THIS GOON NOW <<<");
    let ticks = seconds / interval;
    for tick in 0..ticks {
        std::thread::sleep(std::time::Duration::from_secs(interval));
        if let Some(v) = factory_call(api, "GetBehaviourState", json!([idx])) {
            let active = v["active_type"].as_str().unwrap_or("null");
            let enabled = v["enabled_count"].as_i64().unwrap_or(0);
            println!("  tick {tick}: active={active} enabled={enabled}");
        } else {
            println!("  tick {tick}: GetBehaviourState failed (NPC dead?)");
            break;
        }
    }
}

#[test]
fn isolate_responses_vs_aggression() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position");
        return;
    };

    // Goon A: base NPCResponses, default Aggression (0.1)
    let Some(idx_a) = spawn_and_configure(
        &api,
        "GOON A (base responses, aggression 0.1)",
        px + 4.0,
        py,
        pz + 4.0,
        true, // swap to base NPCResponses
        None, // keep default 0.1
    ) else {
        return;
    };

    // Goon B: NPCResponses_Civilian (default), Aggression 1.0
    let Some(idx_b) = spawn_and_configure(
        &api,
        "GOON B (civilian responses, aggression 1.0)",
        px - 4.0,
        py,
        pz + 4.0,
        false, // keep NPCResponses_Civilian
        Some(1.0),
    ) else {
        return;
    };

    println!("\n=============================================");
    println!("TWO GOONS READY. PUNCH EACH ONE AND OBSERVE.");
    println!("  Goon A (right): base NPCResponses, agg 0.1");
    println!("  Goon B (left):  civilian responses, agg 1.0");
    println!("=============================================");

    // Watch both in alternating ticks for 60s
    println!("\n=== Watching both for 60s (punch them!) ===");
    for tick in 0..12 {
        std::thread::sleep(std::time::Duration::from_secs(5));

        let mut line_a = String::from("???");
        let mut line_b = String::from("???");

        if let Some(v) = factory_call(&api, "GetBehaviourState", json!([idx_a])) {
            let active = v["active_type"].as_str().unwrap_or("null");
            let enabled = v["enabled_count"].as_i64().unwrap_or(0);
            line_a = format!("active={active} enabled={enabled}");
        }

        if let Some(v) = factory_call(&api, "GetBehaviourState", json!([idx_b])) {
            let active = v["active_type"].as_str().unwrap_or("null");
            let enabled = v["enabled_count"].as_i64().unwrap_or(0);
            line_b = format!("active={active} enabled={enabled}");
        }

        println!("  tick {tick}: A(base resp)=[{line_a}]  B(civ resp+agg1)=[{line_b}]");
    }
}
