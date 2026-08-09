//! Test: enable both idle and combat on a custom goon,
//! then the operator punches it to see if it fights back.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_behaviour_list. --test-threads=1 --nocapture
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
fn combat_enabled_goon_retaliates() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position; load a save first");
        return;
    };
    println!("player at ({px:.1}, {py:.1}, {pz:.1})");

    // spawn right in front of the player
    let sx = px + 2.0;
    let sz = pz + 1.0;
    println!("spawning goon at ({sx:.1}, {py:.1}, {sz:.1})");

    let spawn = factory_call(&api, "SpawnGoon", json!([sx, py, sz]));
    let Some(ref sv) = spawn else {
        println!("SpawnGoon failed");
        return;
    };
    let idx = sv["index"].as_i64().unwrap_or(0);
    println!("spawned goon index={idx}");

    println!("waiting 10s for spawn pipeline...");
    std::thread::sleep(std::time::Duration::from_secs(10));

    // enable idle (hold post)
    let idle = factory_call(&api, "EnableIdleBehaviour", json!([idx]));
    match &idle {
        Some(v) => println!("idle enabled: active={}", v["active_type"]),
        None => println!("EnableIdleBehaviour failed"),
    }

    // enable combat (so it fights back)
    let combat = factory_call(&api, "EnableCombatBehaviour", json!([idx]));
    match &combat {
        Some(v) => {
            println!("combat enabled: active={}", v["active_type"]);
            println!("enabled count: {}", v["enabled_count"]);
            if let Some(list) = v["enabled_list"].as_array() {
                for b in list {
                    println!("  enabled: {} pri={}", b["type"], b["pri"]);
                }
            }
        }
        None => println!("EnableCombatBehaviour failed"),
    }

    println!("\n=== PUNCH THE GOON NOW (2m in front of you) ===");
    println!("polling for 90s...");

    for tick in 0..30 {
        std::thread::sleep(std::time::Duration::from_secs(3));
        let s = factory_call(&api, "GetBehaviourState", json!([idx]));
        if let Some(v) = &s {
            let active = v["active_type"].as_str().unwrap_or("null");
            let enabled = v["enabled_count"].as_i64().unwrap_or(0);
            println!("tick {tick} ({}s): active={active} enabled={enabled}", (tick + 1) * 3);
            if active.contains("Combat") {
                println!("FACT: CombatBehaviour activated. goon is fighting back!");
                break;
            }
            if active.contains("Dead") || active.contains("Unconscious") {
                println!("goon went down at tick {tick}");
                break;
            }
        } else {
            println!("tick {tick}: NPC gone");
            break;
        }
    }
}
