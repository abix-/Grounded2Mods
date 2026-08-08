//! Research question 1 (docs/research.md): what class owns the
//! town's map regions and what state it carries.
//!
//! Candidates found 2026-08-07 by metadata scan of the interop
//! Assembly-CSharp.dll (see docs/certainty-tracking.md):
//! - ScheduleOne.Map.Map (fields Regions, RegionDict; type
//!   MapRegionData; enum EMapRegion)
//! - ScheduleOne.Cartel.CartelInfluence (GetInfluence(EMapRegion),
//!   ChangeInfluence, RegionInfluence, DefaultRegionInfluence)
//!
//! This test proves them live: walk each class, inspect the first
//! instance, print its fields. Run with the game up and
//! schedule1_mod loaded:
//!
//! ```text
//! cargo test -p schedule1-mod --test research_map. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running so
//! the workspace suite stays green.

use modforge::client::Api;
use serde_json::{Value, json};

fn api() -> Api<Value> {
    let port = std::env::var("SCHEDULE1_MOD_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(17175);
    Api::at(port, "/op")
}

/// walk_class trying the interop name first, then the plain name.
fn walk(api: &Api<Value>, class: &str) -> Option<Vec<Value>> {
    for name in [format!("Il2Cpp{class}"), class.to_string()] {
        let walk = api.op("walk_class", json!({"class": name}));
        if walk.ok {
            let instances = walk.result.as_array().cloned().unwrap_or_default();
            println!("walk_class({name}): {} instance(s)", instances.len());
            return Some(instances);
        }
        println!("walk_class({name}) failed: {:?}", walk.error);
    }
    None
}

fn dump_first(api: &Api<Value>, class: &str) {
    let Some(instances) = walk(api, class) else {
        println!("{class}: NOT resolvable live");
        return;
    };
    let Some(handle) = instances.first().and_then(|i| i["handle"].as_i64()) else {
        println!("{class}: resolvable but zero live instances (scene-dependent?)");
        return;
    };
    let inspect = api.op("inspect_object", json!({"handle": handle}));
    if !inspect.ok {
        println!("{class}: inspect_object failed: {:?}", inspect.error);
        return;
    }
    println!(
        "{class} fields:\n{}",
        serde_json::to_string_pretty(&inspect.result).unwrap_or_default()
    );
}

#[test]
fn map_region_owner() {
    let api = api();

    let ping = match api.try_op("ping", json!({})) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("SKIP: no control plane answering ({e}); launch the game with schedule1_mod loaded");
            return;
        }
    };
    assert!(ping.ok, "ping not ok: {:?}", ping.error);

    dump_first(&api, "ScheduleOne.Map.Map");
    dump_first(&api, "ScheduleOne.Cartel.CartelInfluence");
}
