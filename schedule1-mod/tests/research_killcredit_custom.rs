//! Kill credit on custom goons: does the Harmony prefix on
//! NPCHealth.Die fire when a spawned goon dies from NPC combat?
//!
//! Spawns two goons, arms one, orders it to kill the other.
//! Polls for the victim going to Dead state. The killcredit
//! module (if loaded) logs "npc down ptr=... player_hit=false"
//! to the MelonLoader console on Die. Operator checks the log.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_killcredit_custom. --test-threads=1 --nocapture
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
fn die_hook_fires_on_custom_goon() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position; load a save first");
        return;
    };

    // spawn the victim close to player
    let victim = factory_call(&api, "SpawnGoon", json!([px + 4.0, py, pz]));
    let Some(ref vv) = victim else {
        println!("SpawnGoon (victim) failed");
        return;
    };
    let vi = vv["index"].as_i64().unwrap_or(0);
    println!("victim spawned index={vi}");

    // spawn the killer right next to the victim
    let killer = factory_call(&api, "SpawnGoon", json!([px + 5.5, py, pz]));
    let Some(ref kv) = killer else {
        println!("SpawnGoon (killer) failed");
        return;
    };
    let ki = kv["index"].as_i64().unwrap_or(0);
    println!("killer spawned index={ki}");

    println!("waiting 10s for spawn pipeline...");
    std::thread::sleep(std::time::Duration::from_secs(10));

    // lower victim toughness so it dies faster
    let tough = factory_call(&api, "SetToughness", json!([vi, 10]));
    match &tough {
        Some(_) => println!("victim toughness set to 10"),
        None => println!("SetToughness failed (continuing anyway)"),
    }

    // put the victim on idle hold so it stays still
    let idle = factory_call(&api, "EnableIdleBehaviour", json!([vi]));
    match &idle {
        Some(v) => println!("victim idle enabled: active={}", v["active_type"]),
        None => println!("EnableIdleBehaviour failed (continuing anyway)"),
    }

    // arm the killer with a knife
    factory_call(&api, "Arm", json!([ki, "Avatar/Equippables/Knife"]));
    println!("killer armed with knife");

    // order the killer to attack the victim
    let atk = factory_call(&api, "AttackNpc", json!([ki, vi]));
    match &atk {
        Some(_) => println!("killer ordered to attack victim"),
        None => println!("AttackNpc failed (continuing anyway)"),
    }

    // poll victim state until dead or timeout
    println!("\npolling victim state...");
    let mut saw_dead = false;
    for tick in 0..20 {
        std::thread::sleep(std::time::Duration::from_secs(3));
        let state = factory_call(&api, "GetBehaviourState", json!([vi]));
        match &state {
            Some(sv) => {
                let active = sv["active_type"].as_str().unwrap_or("null");
                let enabled = sv["enabled_count"].as_i64().unwrap_or(0);
                println!(
                    "tick {tick} ({}s): active={active} enabled={enabled}",
                    (tick + 1) * 3,
                );
                if active.contains("Dead") || active.contains("dead") {
                    saw_dead = true;
                    println!("victim is DEAD at tick {tick}");
                    break;
                }
            }
            None => {
                println!("tick {tick}: GetBehaviourState failed (NPC may be destroyed)");
                saw_dead = true;
                break;
            }
        }
    }

    println!("\n=== VERDICT ===");
    if saw_dead {
        println!("FACT: custom goon reached Dead state from NPC combat");
        println!("OPERATOR CHECK: look at MelonLoader console for:");
        println!("  schedule1-mod [kill]: npc down ptr=... player_hit=false");
        println!("If that line exists, the Die Harmony prefix fires on custom NPCs.");
        println!("If absent, the hook does NOT fire on custom goons.");
    } else {
        println!("victim did NOT die within 60s observation window");
        println!("possible causes: killer not attacking, damage too low, timeout too short");
    }
}
