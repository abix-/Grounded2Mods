//! Where did the hit squad fighters actually end up?
//!
//! Lists every live FighterHandler on team "HitSquad": position,
//! distance to the player, health, current action.
//!
//! ```text
//! cargo test -p bossgangsters-mod --test research_hit_squad_check -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, ping_or_skip};
use serde_json::json;

#[test]
fn where_are_the_hit_squad_fighters() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let handle_in = |res: &serde_json::Value| res.get("handle").and_then(|h| h.as_i64());
    let vec3 = |v: &serde_json::Value| {
        (
            v.get("x").and_then(|c| c.as_f64()).unwrap_or(0.0),
            v.get("y").and_then(|c| c.as_f64()).unwrap_or(0.0),
            v.get("z").and_then(|c| c.as_f64()).unwrap_or(0.0),
        )
    };

    let club = unityforge::client::first_handle_inactive(&api, "ClubPlayer").expect("no ClubPlayer");
    let fh = api.op("read_field", json!({"handle": club, "field": "playerFighterHandler"}));
    let player = handle_in(&fh.result).expect("no player fighter");
    let pbot = handle_in(
        &api.op("invoke_method", json!({"handle": player, "method": "GetBot", "args": []}))
            .result,
    )
    .expect("no player bot");
    let ptr = handle_in(&api.op("read_field", json!({"handle": pbot, "field": "transform"})).result)
        .expect("no player transform");
    let ppos = vec3(&api.op("read_field", json!({"handle": ptr, "field": "position"})).result);
    println!("player at {ppos:?}");

    let res = api.op(
        "walk_class",
        json!({"class": "FighterHandler", "include_inactive": true}),
    );
    assert!(res.ok, "walk_class failed: {:?}", res.error);
    let instances = res.result["instances"].as_array().cloned().unwrap_or_default();
    println!("{} FighterHandler instances", instances.len());
    let mut found = 0;
    for inst in &instances {
        let Some(h) = inst.get("handle").and_then(|x| x.as_i64()) else { continue };
        let team = api.op("read_field", json!({"handle": h, "field": "teamName"}));
        if team.result.as_str() != Some("HitSquad") {
            continue;
        }
        found += 1;
        let bot = handle_in(
            &api.op("invoke_method", json!({"handle": h, "method": "GetBot", "args": []})).result,
        );
        let mut pos = (0.0, 0.0, 0.0);
        let mut action = String::from("?");
        if let Some(b) = bot {
            if let Some(t) =
                handle_in(&api.op("read_field", json!({"handle": b, "field": "transform"})).result)
            {
                pos = vec3(&api.op("read_field", json!({"handle": t, "field": "position"})).result);
            }
            let a = api.op("read_field", json!({"handle": b, "field": "actionBase"}));
            if let Some(ah) = handle_in(&a.result) {
                let dump = api.op("inspect_object", json!({"handle": ah}));
                action = dump.result["class_name"]
                    .as_str()
                    .unwrap_or("?")
                    .to_string();
            }
        }
        let health = api.op("read_field", json!({"handle": h, "field": "Health"}));
        let hp = handle_in(&health.result)
            .map(|hh| api.op("read_field", json!({"handle": hh, "field": "health"})).result)
            .unwrap_or_default();
        let d = (((pos.0 - ppos.0).powi(2) + (pos.2 - ppos.2).powi(2)) as f64).sqrt();
        println!(
            "HitSquad fighter {h}: pos {pos:?} distance {d:.1} action {action} health {hp}"
        );
    }
    println!("HitSquad fighters found: {found}");
}
