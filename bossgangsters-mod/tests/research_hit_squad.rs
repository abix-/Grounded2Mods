//! Does the hit_squad op spawn family fighters that come for
//! the player?
//!
//! Drives the mod's `hit_squad` op: 2 Vice Family fighters,
//! weapon tier 1, spawned in a ring around the player with the
//! player as their fight target. The proof is on screen (they
//! attack unprovoked) plus the op result and log line.
//!
//! ```text
//! cargo test -p bossgangsters-mod --test research_hit_squad -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, ping_or_skip};
use serde_json::json;

#[test]
fn hit_squad_spawns_and_attacks() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let res = api.op(
        "hit_squad",
        json!({"family": "ViceFamily", "size": 2, "weapon_tier": 1}),
    );
    if res.ok {
        println!("hit squad: {}", res.result);
    } else {
        println!("hit squad FAILED: {:?}", res.error);
    }
    assert!(res.ok, "hit_squad op failed: {:?}", res.error);
}
