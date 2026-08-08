//! Research: how to make a stationed goon STAY at a post.
//! Untasked goons walk to exit buildings (proven), and the
//! garrison's SetDestination orders failed silently, so every
//! mob piled up at an exit. Find the call that holds them:
//! 1. CartelGoon methods that smell like guard/stay/patrol.
//! 2. SetDestination on the goon's NPCMovement: print the real
//!    error / result per arg shape.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_hold. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, first_handle, handle_of, ping_or_skip};
use serde_json::json;

/// The vanilla ambush machinery: goons standing at a location
/// until the player comes close is EXACTLY garrison semantics.
/// What do Ambush / CartelActivities / CartelAmbushLocation
/// declare, and where are the ambush locations?
///
/// ```text
/// cargo test -p schedule1-mod --test research_hold ambush_machinery. --test-threads=1 --nocapture
/// ```
#[test]
fn ambush_machinery() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    for class in [
        "Il2CppScheduleOne.Cartel.Ambush",
        "Il2CppScheduleOne.Cartel.CartelActivities",
        "Il2CppScheduleOne.Cartel.CartelAmbushLocation",
    ] {
        common::print_declared_methods(&api, class);
    }
    if let Some(instances) = common::walk(&api, "ScheduleOne.Cartel.CartelAmbushLocation") {
        for (i, inst) in instances.iter().enumerate().take(10) {
            let Some(h) = inst["handle"].as_i64() else { continue };
            let t = api.op("read_field", json!({"handle": h, "field": "transform"}));
            if let Some(th) = handle_of(&t.result) {
                let p = api.op("invoke_method", json!({"handle": th, "method": "get_position", "args": []}));
                println!("ambush location [{i}] {}: {:?}", inst["name"], common::parse_vec3(&p.result));
            }
        }
    }
}

/// Fire the vanilla ambush machinery once, live: walk the
/// Ambush activity instance, pick the ambush location nearest
/// the player, and try SpawnAmbush arg shapes until one takes.
/// OPERATOR: watch for goons materializing at that spot and
/// standing their ground.
///
/// ```text
/// cargo test -p schedule1-mod --test research_hold spawn_one_ambush. --test-threads=1 --nocapture
/// ```
#[test]
fn spawn_one_ambush() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some((px, _, pz)) = common::player_position(&api) else {
        println!("no player position; not in a save?");
        return;
    };
    let Some(ambush) = first_handle(&api, "ScheduleOne.Cartel.Ambush") else {
        println!("no live Ambush activity instance");
        return;
    };
    let Some(locations) = common::walk(&api, "ScheduleOne.Cartel.CartelAmbushLocation") else {
        return;
    };
    // Nearest location to the player, so the operator can watch.
    let mut best: Option<(i64, f64, (f64, f64, f64))> = None;
    for inst in &locations {
        let Some(h) = inst["handle"].as_i64() else { continue };
        let t = api.op("read_field", json!({"handle": h, "field": "transform"}));
        let Some(th) = handle_of(&t.result) else { continue };
        let p = api.op("invoke_method", json!({"handle": th, "method": "get_position", "args": []}));
        api.op("release_handle", json!({"handle": th}));
        let Some((x, y, z)) = common::parse_vec3(&p.result) else { continue };
        let d = ((x - px).powi(2) + (z - pz).powi(2)).sqrt();
        if best.is_none_or(|(_, bd, _)| d < bd) {
            best = Some((h, d, (x, y, z)));
        }
    }
    let Some((loc, dist, pos)) = best else {
        println!("no ambush location resolved");
        return;
    };
    println!("nearest ambush location: {dist:.0}m away at {pos:?}");

    // Signature (from live probe errors 2026-08-08):
    // SpawnAmbush(Player, Vector3[]). Feed it the location's
    // AmbushPoints positions.
    let mut points = Vec::new();
    let ap = api.op("read_field", json!({"handle": loc, "field": "AmbushPoints"}));
    if let Some(aph) = handle_of(&ap.result) {
        let n = common::count_of(&api, aph).unwrap_or(0);
        for j in 0..n {
            let e = api.op("invoke_method", json!({"handle": aph, "method": "get_Item", "args": [j]}));
            if let Some(eh) = handle_of(&e.result) {
                let p = api.op("invoke_method", json!({"handle": eh, "method": "get_position", "args": []}));
                if let Some((x, y, z)) = common::parse_vec3(&p.result) {
                    points.push(json!({"x": x, "y": y, "z": z}));
                }
                api.op("release_handle", json!({"handle": eh}));
            }
        }
    }
    println!("{} ambush point(s) at the location", points.len());
    if points.is_empty() {
        return;
    }
    let Some(player) = first_handle(&api, "ScheduleOne.PlayerScripts.Player") else {
        return;
    };
    let r = api.op(
        "invoke_method",
        json!({"handle": ambush, "method": "SpawnAmbush",
               "args": [{"$handle": player}, points]}),
    );
    println!("SpawnAmbush(player, points): ok={} result={} err={:?}", r.ok, r.result, r.error);
    if r.ok {
        println!("OPERATOR CHECK: goons standing at {pos:?} ({dist:.0}m away)? Do they hold until you get close? Are they ARMED?");
    }
}

#[test]
fn how_to_hold_a_goon() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // Guard-flavored surface on the goon + its behaviour brain.
    let r = api.op("list_methods", json!({"class": "Il2CppScheduleOne.Cartel.CartelGoon"}));
    if r.ok {
        for m in r.result["methods"].as_array().cloned().unwrap_or_default() {
            let name = m["name"].as_str().unwrap_or("");
            if name.starts_with("RpcReader") || name.starts_with("RpcWriter") {
                continue;
            }
            let hit = ["Guard", "Patrol", "Stay", "Post", "Idle", "Stationary", "Behaviour", "Wander"]
                .iter()
                .any(|w| name.contains(w));
            if hit {
                println!(
                    "  CartelGoon :: {}({}) -> {} [{}]",
                    name,
                    m["params"].as_i64().unwrap_or(-1),
                    m["return"].as_str().unwrap_or("?"),
                    m["declared_on"].as_str().unwrap_or("?"),
                );
            }
        }
    }

    // A live goon's movement: what does SetDestination actually
    // say per arg shape?
    let Some(goon) = first_handle(&api, "ScheduleOne.Cartel.CartelGoon") else {
        println!("no live CartelGoon (garrisons empty?)");
        return;
    };
    let mv = api.op("read_field", json!({"handle": goon, "field": "Movement"}));
    let Some(mh) = handle_of(&mv.result) else {
        println!("goon Movement carried no handle: {}", mv.result);
        return;
    };
    for args in [
        json!([{"x": 100.0, "y": 0.0, "z": 100.0}]),
        json!([{"x": 100.0, "y": 0.0, "z": 100.0}, 1.0]),
    ] {
        let r = api.op("invoke_method", json!({"handle": mh, "method": "SetDestination", "args": args}));
        println!("SetDestination {args}: ok={} result={} err={:?}", r.ok, r.result, r.error);
    }
}
