//! Research for the mob-farming slice: the affix anchor points.
//!
//! 1. NPCMovement: the speed field for a Swift mob type.
//! 2. NPCHealth on a SPAWNED GOON: are MaxHealth and Health
//!    writable (Tough mob type)? Never probe writes on civilian
//!    NPCs; a goon is disposable.
//!
//! Run with the operator in-game. Spawns one goon 8m away,
//! probes it, then orders it onto the player (it would walk to
//! an exit otherwise).
//!
//! ```text
//! cargo test -p schedule1-mod --test research_farming. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, first_handle, handle_of, ping_or_skip, player_position, print_declared_methods};
use serde_json::json;

#[test]
fn farming_anchor_points() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    print_declared_methods(&api, "Il2CppScheduleOne.NPCs.NPCMovement");

    let Some((px, py, pz)) = player_position(&api) else {
        println!("no player position; not in a save?");
        return;
    };
    let Some(pool) = first_handle(&api, "ScheduleOne.Cartel.GoonPool") else {
        return;
    };
    let spawn = api.op(
        "invoke_method",
        json!({"handle": pool, "method": "SpawnGoon",
               "args": [{"x": px + 8.0, "y": py, "z": pz}]}),
    );
    let Some(goon) = handle_of(&spawn.result) else {
        println!("SpawnGoon failed: {:?}", spawn.error);
        return;
    };
    println!("goon spawned for probing");

    // Its NPCHealth: read, write MaxHealth + Health up, read back.
    let health = api.op("read_field", json!({"handle": goon, "field": "Health"}));
    let Some(hh) = handle_of(&health.result) else {
        println!("goon Health field carried no handle: {}", health.result);
        return;
    };
    for field in ["MaxHealth", "Health"] {
        let before = api.op("read_field", json!({"handle": hh, "field": field}));
        let w = api.op(
            "write_field",
            json!({"handle": hh, "field": field, "value": 250.0}),
        );
        let after = api.op("read_field", json!({"handle": hh, "field": field}));
        println!(
            "goon NPCHealth.{field}: before={} write_ok={} after={}",
            before.result, w.ok, after.result
        );
    }

    // Its movement: find the speed-ish members live.
    let movement = api.op("read_field", json!({"handle": goon, "field": "Movement"}));
    match handle_of(&movement.result) {
        Some(mh) => {
            for field in [
                "MoveSpeedMultiplier",
                "WalkSpeed",
                "RunSpeed",
                "SpeedController",
            ] {
                let r = api.op("read_field", json!({"handle": mh, "field": field}));
                println!("goon Movement.{field}: ok={} {}", r.ok, r.result);
            }
        }
        None => println!("goon Movement field carried no handle: {}", movement.result),
    }

    // Task it so it does not wander off; the operator can fight
    // it (it now has 250 health if the write landed).
    if let Some(player) = first_handle(&api, "ScheduleOne.PlayerScripts.Player") {
        let attack = api.op(
            "invoke_method",
            json!({"handle": goon, "method": "AttackEntity",
                   "args": [{"$handle": player}, true]}),
        );
        println!(
            "AttackEntity ok={} (fight it to feel the 250 health)",
            attack.ok
        );
    }
}
