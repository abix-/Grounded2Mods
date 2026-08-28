//! Priority resolution: does CombatBehaviour override
//! IdleBehaviour when a goon on idle-hold gets attacked?
//! Does idle resume after combat ends?
//!
//! ```text
//! cargo test -p schedule1-mod --test research_priority -- --test-threads=1 --nocapture
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
            println!("{method}: bad json: {e}");
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
fn combat_overrides_idle_then_resumes() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position; load a save first");
        return;
    };

    // spawn the guard (will be put on idle hold)
    let guard = factory_call(&api, "SpawnGoon", json!([px + 6.0, py, pz]));
    let Some(ref gv) = guard else {
        println!("SpawnGoon (guard) failed");
        return;
    };
    let gi = gv["index"].as_i64().unwrap_or(0);
    println!("guard spawned index={gi}");

    // spawn the attacker nearby
    let attacker = factory_call(&api, "SpawnGoon", json!([px + 10.0, py, pz]));
    let Some(ref av) = attacker else {
        println!("SpawnGoon (attacker) failed");
        return;
    };
    let ai = av["index"].as_i64().unwrap_or(0);
    println!("attacker spawned index={ai}");

    println!("waiting 10s for spawn pipeline...");
    std::thread::sleep(std::time::Duration::from_secs(10));

    // put the guard on idle hold
    let idle = factory_call(&api, "EnableIdleBehaviour", json!([gi]));
    match &idle {
        Some(v) => println!("guard idle enabled: active={}", v["active_type"]),
        None => {
            println!("EnableIdleBehaviour failed on guard");
            return;
        }
    }

    // arm the attacker so combat is visible
    factory_call(&api, "Arm", json!([ai, "Avatar/Equippables/Knife"]));

    // order the attacker to attack the guard
    let atk = factory_call(&api, "AttackNpc", json!([ai, gi]));
    match &atk {
        Some(_) => println!("attacker ordered to attack guard"),
        None => println!("AttackNpc failed (continuing anyway)"),
    }

    // poll the guard's active behaviour during the fight
    println!("\npolling guard behaviour during fight...");
    let mut saw_combat = false;
    let mut saw_idle_resume = false;
    for tick in 0..12 {
        std::thread::sleep(std::time::Duration::from_secs(3));
        let state = factory_call(&api, "GetBehaviourState", json!([gi]));
        let Some(ref sv) = state else {
            println!("tick {tick}: GetBehaviourState failed");
            continue;
        };
        let active = sv["active_type"].as_str().unwrap_or("null");
        let enabled = sv["enabled_count"].as_i64().unwrap_or(0);
        let px = sv["pos_x"].as_f64().unwrap_or(0.0);
        let pz = sv["pos_z"].as_f64().unwrap_or(0.0);
        println!(
            "tick {tick} ({}s): active={} enabled={} pos=({:.1}, {:.1})",
            (tick + 1) * 3,
            active,
            enabled,
            px,
            pz
        );

        if active.contains("Combat") {
            saw_combat = true;
        }
        if saw_combat && active.contains("Idle") {
            saw_idle_resume = true;
        }
    }

    println!("\n=== VERDICT ===");
    if saw_combat {
        println!("FACT: CombatBehaviour overrides IdleBehaviour when attacked");
    } else {
        println!("combat did NOT override idle (guard stayed on idle the whole time)");
    }
    if saw_idle_resume {
        println!("FACT: IdleBehaviour resumes after combat ends");
    } else if saw_combat {
        println!("combat fired but idle did NOT resume within 36s observation window");
    }
    println!("OPERATOR CHECK: did the guard fight back, then return to standing still?");
}
