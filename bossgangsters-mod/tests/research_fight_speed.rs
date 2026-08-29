//! Is the Fight speed multiplier applied to the live player?
//!
//! Expected: runSpeed = BotManager base x (1 + 0.01 x Fight).
//!
//! ```text
//! cargo test -p bossgangsters-mod --test research_fight_speed -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, ping_or_skip};
use serde_json::json;

#[test]
fn player_speed_reflects_fight_level() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let handle_in = |res: &serde_json::Value| res.get("handle").and_then(|h| h.as_i64());

    let Some(club_player) = unityforge::client::first_handle_inactive(&api, "ClubPlayer") else {
        println!("ClubPlayer: no live instance; load a save first");
        return;
    };
    let fh = api.op(
        "read_field",
        json!({"handle": club_player, "field": "playerFighterHandler"}),
    );
    let fighter = handle_in(&fh.result).expect("no fighter handle");
    let fight = api.op(
        "invoke_method",
        json!({"handle": fighter, "method": "GetSkillLevel", "args": [1]}),
    );
    let bot = api.op("invoke_method", json!({"handle": fighter, "method": "GetBot", "args": []}));
    let bot = handle_in(&bot.result).expect("no bot handle");
    let walk = api.op("read_field", json!({"handle": bot, "field": "walkSpeed"}));
    let run = api.op("read_field", json!({"handle": bot, "field": "runSpeed"}));
    println!(
        "Fight {} -> walkSpeed {} runSpeed {} (vanilla 2.8 / 4.5)",
        fight.result, walk.result, run.result
    );
}
