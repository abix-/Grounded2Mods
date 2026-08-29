//! Is the food regen actually ticking health up?
//!
//! Reads the player's health twice, 4 seconds apart, while a
//! food regen buff is active. Below max health the second read
//! should be ~4 higher (1 HP per second).
//!
//! ```text
//! cargo test -p bossgangsters-mod --test research_food_regen -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, ping_or_skip};
use serde_json::json;

#[test]
fn health_climbs_under_regen() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let Some(club_player) = unityforge::client::first_handle_inactive(&api, "ClubPlayer") else {
        println!("ClubPlayer: no live instance; load a save first");
        return;
    };
    let fh = api.op(
        "read_field",
        json!({"handle": club_player, "field": "playerFighterHandler"}),
    );
    assert!(fh.ok, "playerFighterHandler read failed: {:?}", fh.error);
    let fighter = fh.result["handle"].as_i64().expect("no fighter handle");

    let read_health = || -> (f64, f64) {
        let h = api.op("read_field", json!({"handle": fighter, "field": "Health"}));
        assert!(h.ok, "Health read failed: {:?}", h.error);
        let health = h.result["handle"].as_i64().expect("no Health handle");
        let hp = api.op("read_field", json!({"handle": health, "field": "health"}));
        let max = api.op("read_field", json!({"handle": health, "field": "maxHealth"}));
        assert!(hp.ok && max.ok, "health fields read failed");
        (hp.result.as_f64().unwrap(), max.result.as_f64().unwrap())
    };

    let (before, max) = read_health();
    println!("health before: {before:.1} / {max:.1}");
    std::thread::sleep(std::time::Duration::from_secs(4));
    let (after, _) = read_health();
    println!("health after 4s: {after:.1}");
    if before >= max {
        println!("player at max health; regen has nothing to do (damage the player to see it climb)");
        return;
    }
    println!("delta: {:+.1} over 4s", after - before);
}
