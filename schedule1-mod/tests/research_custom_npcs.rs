//! Custom NPC minting verification (docs/todo.md,
//! "Custom NPCs"): the shim's S1API-backed NpcFactory, driven
//! through invoke_static. Spawns three goons, one police, one
//! player NPC in a line next to the player.
//!
//! Run with the operator in-game and watching.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_custom_npcs. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, ping_or_skip, player_position};
use serde_json::json;

const FACTORY: &str = "Unityforge.Shim.Schedule1.NpcFactory";

#[test]
fn mint_custom_npcs() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let count = api.op(
        "invoke_static",
        json!({"class": FACTORY, "method": "CustomNpcCount", "args": []}),
    );
    println!("CustomNpcCount: ok={} {}", count.ok, count.result);
    if !count.ok {
        println!(
            "factory unreachable (S1API missing or shim stale?): {:?}",
            count.error
        );
        return;
    }
    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position; load a save first");
        return;
    };

    let spawns: [(&str, f64); 5] = [
        ("SpawnGoon", 4.0),
        ("SpawnGoon", 6.0),
        ("SpawnGoon", 8.0),
        ("SpawnPolice", 10.0),
        ("SpawnPlayerNpc", 12.0),
    ];
    for (method, dx) in spawns {
        let r = api.op(
            "invoke_static",
            json!({"class": FACTORY, "method": method,
                   "args": [px + dx, py, pz]}),
        );
        println!("{method} at +{dx}m: ok={} {}", r.ok, r.result);
        std::thread::sleep(std::time::Duration::from_secs(2));
        match api.try_op("ping", json!({})) {
            Ok(p) if p.ok => {}
            _ => {
                println!("GAME DIED on {method}");
                return;
            }
        }
    }
    let count = api.op(
        "invoke_static",
        json!({"class": FACTORY, "method": "CustomNpcCount", "args": []}),
    );
    println!("CustomNpcCount after: {}", count.result);
    println!(
        "OPERATOR CHECK: five new people in a line beside you (3 goons, 1 cop, 1 soldier)? Visible, solid, alive?"
    );
}
