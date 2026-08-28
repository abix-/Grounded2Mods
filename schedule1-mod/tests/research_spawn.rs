//! Research: FIRST LIVE MUTATION. Spawn one cartel goon near the
//! player via the vanilla GoonPool, proving the farmable-mob loop
//! end to end (docs/research.md, mob spawning).
//!
//! Run ONLY with the operator in-game and watching: a hostile
//! goon appears ~3m from the player.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_spawn. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, count_of, first_handle, handle_of, parse_vec3, ping_or_skip};
use serde_json::json;

#[test]
fn spawn_one_goon_near_player() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // Player position via transform.get_position.
    let Some(player) = first_handle(&api, "ScheduleOne.PlayerScripts.Player") else {
        println!("no Player instance; not in a save?");
        return;
    };
    let transform = api.op(
        "read_field",
        json!({"handle": player, "field": "transform"}),
    );
    let Some(th) = handle_of(&transform.result) else {
        println!("player transform carried no handle: {}", transform.result);
        return;
    };
    let pos = api.op(
        "invoke_method",
        json!({"handle": th, "method": "get_position", "args": []}),
    );
    let Some((px, py, pz)) = parse_vec3(&pos.result) else {
        println!("could not parse player position: {}", pos.result);
        return;
    };
    println!("player at ({px:.1}, {py:.1}, {pz:.1})");

    // Goon pool count before.
    let Some(pool) = first_handle(&api, "ScheduleOne.Cartel.GoonPool") else {
        return;
    };
    let before = api.op(
        "read_field",
        json!({"handle": pool, "field": "spawnedGoons"}),
    );
    let n_before = handle_of(&before.result).and_then(|h| count_of(&api, h));
    println!("spawnedGoons before: {n_before:?}");

    // Spawn goons near the player. SCHEDULE1_SPAWN_COUNT sets how
    // many (default 1); each lands on a small ring around the
    // player so they don't stack.
    let count: usize = std::env::var("SCHEDULE1_SPAWN_COUNT")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(1);
    for i in 0..count {
        let angle = (i as f64) * 2.1;
        let (dx, dz) = (3.0 * angle.cos() + 2.0, 3.0 * angle.sin() + 2.0);
        let spawn = api.op(
            "invoke_method",
            json!({"handle": pool, "method": "SpawnGoon",
                   "args": [{"x": px + dx, "y": py, "z": pz + dz}]}),
        );
        if spawn.ok {
            println!("SpawnGoon[{i}] returned: {}", spawn.result);
        } else {
            println!("SpawnGoon[{i}] FAILED: {:?}", spawn.error);
            return;
        }
    }

    let after = api.op(
        "read_field",
        json!({"handle": pool, "field": "spawnedGoons"}),
    );
    let n_after = handle_of(&after.result).and_then(|h| count_of(&api, h));
    println!("spawnedGoons after: {n_after:?}");
    println!("OPERATOR CHECK: a goon should be ~3m from you. Report what you see.");
}
