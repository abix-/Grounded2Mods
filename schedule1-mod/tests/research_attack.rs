//! Research: task a spawned goon to attack the player, proving
//! the full farmable-mob loop (spawn -> aggro -> fight) and the
//! {"$handle": N} live-object argument path.
//!
//! Run ONLY with the operator in-game, watching, and ready to
//! fight: a hostile goon spawns ~5m away and is ordered to
//! attack via CartelGoon.AttackEntity(ICombatTargetable, bool).
//!
//! ```text
//! cargo test -p schedule1-mod --test research_attack. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, first_handle, handle_of, parse_vec3, ping_or_skip};
use serde_json::json;

#[test]
fn goon_attacks_player() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let Some(player) = first_handle(&api, "ScheduleOne.PlayerScripts.Player") else {
        println!("no Player instance; not in a save?");
        return;
    };
    let transform = api.op("read_field", json!({"handle": player, "field": "transform"}));
    let Some(th) = handle_of(&transform.result) else {
        println!("player transform carried no handle: {}", transform.result);
        return;
    };
    let pos = api.op("invoke_method", json!({"handle": th, "method": "get_position", "args": []}));
    let Some((px, py, pz)) = parse_vec3(&pos.result) else {
        println!("could not parse player position: {}", pos.result);
        return;
    };
    println!("player at ({px:.1}, {py:.1}, {pz:.1})");

    let Some(pool) = first_handle(&api, "ScheduleOne.Cartel.GoonPool") else {
        return;
    };
    let spawn = api.op(
        "invoke_method",
        json!({"handle": pool, "method": "SpawnGoon",
               "args": [{"x": px + 5.0, "y": py, "z": pz}]}),
    );
    let Some(goon) = handle_of(&spawn.result) else {
        println!("SpawnGoon failed or carried no handle: {:?} {}", spawn.error, spawn.result);
        return;
    };
    println!("goon spawned: {}", spawn.result);

    // The order: attack the player. Second arg per the metadata
    // signature AttackEntity(ICombatTargetable, Boolean).
    let attack = api.op(
        "invoke_method",
        json!({"handle": goon, "method": "AttackEntity",
               "args": [{"$handle": player}, true]}),
    );
    if attack.ok {
        println!("AttackEntity ok: {}", attack.result);
        println!("OPERATOR CHECK: the goon should be hunting you. Report the fight.");
    } else {
        println!("AttackEntity FAILED: {:?}", attack.error);
    }
}
