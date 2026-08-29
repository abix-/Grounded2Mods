//! What do the LIVE territories and prestige look like?
//!
//! Reads TerritoryManager's territory list (name, star level,
//! owner, claim phase) and the player's prestige points.
//!
//! ```text
//! cargo test -p bossgangsters-mod --test research_gangs -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, ping_or_skip};
use serde_json::json;

#[test]
fn territories_and_prestige() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let handle_in = |res: &serde_json::Value| res.get("handle").and_then(|h| h.as_i64());

    if let Some(prestige) = unityforge::client::first_handle_inactive(&api, "PlayerPrestigeManager")
    {
        let p = api.op(
            "read_field",
            json!({"handle": prestige, "field": "CurrentPrestigePoints"}),
        );
        println!("prestige points = {}", p.result);
    }

    let Some(tm) = unityforge::client::first_handle_inactive(&api, "TerritoryManager") else {
        println!("TerritoryManager: no live instance");
        return;
    };
    let count = api.op("read_field", json!({"handle": tm, "field": "TerritoryCount"}));
    println!("territories: {}", count.result);
    let n = count.result.as_i64().unwrap_or(0);
    for i in 0..n {
        let def = api.op(
            "invoke_method",
            json!({"handle": tm, "method": "GetTerritoryDefinition", "args": [i]}),
        );
        let state = api.op(
            "invoke_method",
            json!({"handle": tm, "method": "GetTerritoryState", "args": [i]}),
        );
        let (Some(def_h), Some(state_h)) = (handle_in(&def.result), handle_in(&state.result))
        else {
            println!("territory {i}: definition or state unreadable");
            continue;
        };
        let name = api.op("read_field", json!({"handle": def_h, "field": "DisplayName"}));
        let stars = api.op("read_field", json!({"handle": def_h, "field": "StarLevel"}));
        let owner = api.op(
            "read_field",
            json!({"handle": state_h, "field": "ownerFamilyType"}),
        );
        let player_owned = api.op(
            "read_field",
            json!({"handle": state_h, "field": "isPlayerOwned"}),
        );
        let phase = api.op(
            "read_field",
            json!({"handle": state_h, "field": "claimPhase"}),
        );
        println!(
            "territory {i}: {} stars {} owner {} playerOwned {} claimPhase {}",
            name.result, stars.result, owner.result, player_owned.result, phase.result
        );
    }
}
