//! Minted-NPC combat verification: two goons are minted near
//! the player, ARMED (one knife, one baton), and ordered to
//! fight EACH OTHER. This is the faction war's substrate:
//! goon-vs-goon combat with weapons, no player involvement.
//!
//! Run with the operator in-game and watching from a few meters.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_npc_war. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, ping_or_skip, player_position};
use serde_json::json;

const FACTORY: &str = "Unityforge.Shim.Schedule1.NpcFactory";

#[test]
fn goon_vs_goon_with_weapons() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position; load a save first");
        return;
    };

    let mut indices = Vec::new();
    for dx in [5.0, 9.0] {
        let r = api.op(
            "invoke_static",
            json!({"class": FACTORY, "method": "SpawnGoon", "args": [px + dx, py, pz]}),
        );
        println!("SpawnGoon +{dx}m: ok={} {}", r.ok, r.result);
        let idx = r
            .result
            .as_str()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| v["index"].as_i64());
        match idx {
            Some(i) => indices.push(i),
            None => {
                println!("no index in spawn result; stopping");
                return;
            }
        }
    }
    println!("waiting 10s for the spawn pipeline to settle...");
    std::thread::sleep(std::time::Duration::from_secs(10));

    for (i, weapon) in [
        (0usize, "Avatar/Equippables/Knife"),
        (1usize, "Avatar/Equippables/Baton"),
    ] {
        let r = api.op(
            "invoke_static",
            json!({"class": FACTORY, "method": "Arm", "args": [indices[i], weapon]}),
        );
        println!("Arm[{i}] {weapon}: ok={} {}", r.ok, r.result);
    }

    let a = api.op(
        "invoke_static",
        json!({"class": FACTORY, "method": "AttackNpc", "args": [indices[0], indices[1]]}),
    );
    println!("AttackNpc 0->1: ok={} {}", a.ok, a.result);
    let b = api.op(
        "invoke_static",
        json!({"class": FACTORY, "method": "AttackNpc", "args": [indices[1], indices[0]]}),
    );
    println!("AttackNpc 1->0: ok={} {}", b.ok, b.result);
    println!("OPERATOR CHECK: two goons fighting each other with weapons, no player involvement?");
}
