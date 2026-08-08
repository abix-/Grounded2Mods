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

/// Walk the class, return the first live instance handle.
/// inspect_object on IL2CPP only shows the interop wrapper's
/// managed fields (isWrapped/pooledPtr, seen live 2026-08-07),
/// so named native fields are read via read_field instead.
fn first_handle(api: &Api<Value>, class: &str) -> Option<i64> {
    let instances = walk(api, class)?;
    let handle = instances.first().and_then(|i| i["handle"].as_i64());
    if handle.is_none() {
        println!("{class}: resolvable but zero live instances (scene-dependent?)");
    }
    handle
}

/// Handle carried by a complex value (attached by the shim's
/// serializer so ops chain generically).
fn handle_of(v: &Value) -> Option<i64> {
    v.get("handle").and_then(Value::as_i64)
}

/// Element count of any Il2Cpp sequence: arrays answer
/// get_Length, lists answer get_Count.
fn count_of(api: &Api<Value>, h: i64) -> Option<i64> {
    for getter in ["get_Length", "get_Count"] {
        let r = api.op("invoke_method", json!({"handle": h, "method": getter, "args": []}));
        if r.ok {
            return r.result.as_i64();
        }
    }
    None
}

/// Walk a sequence handle generically: get_Item(i) per element,
/// inspect each element, print its fields, release the handles.
fn dump_sequence(api: &Api<Value>, label: &str, seq: i64) {
    let Some(n) = count_of(api, seq) else {
        println!("{label}: no get_Length/get_Count answered");
        return;
    };
    println!("{label}: {n} element(s)");
    for i in 0..n {
        let item = api.op("invoke_method", json!({"handle": seq, "method": "get_Item", "args": [i]}));
        if !item.ok {
            println!("{label}[{i}]: get_Item failed: {:?}", item.error);
            continue;
        }
        let Some(eh) = handle_of(&item.result) else {
            println!("{label}[{i}] = {}", item.result);
            continue;
        };
        let inspect = api.op("inspect_object", json!({"handle": eh}));
        println!("{label}[{i}]:\n{}",
            serde_json::to_string_pretty(&inspect.result).unwrap_or_default());
        api.op("release_handle", json!({"handle": eh}));
    }
    api.op("release_handle", json!({"handle": seq}));
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

    // The full region table, walked generically: Regions is a
    // MapRegionData[]; each element's inspect names the region
    // (Name + Region enum value), closing the EMapRegion mapping.
    if let Some(h) = first_handle(&api, "ScheduleOne.Map.Map") {
        let regions = api.op("read_field", json!({"handle": h, "field": "Regions"}));
        match handle_of(&regions.result) {
            Some(seq) => dump_sequence(&api, "Map.Regions", seq),
            None => println!("Map.Regions carried no handle: {}", regions.result),
        }
    }

    // Influence per region, plus the live influence list.
    if let Some(h) = first_handle(&api, "ScheduleOne.Cartel.CartelInfluence") {
        let infl = api.op("read_field", json!({"handle": h, "field": "regionInfluence"}));
        match handle_of(&infl.result) {
            Some(seq) => dump_sequence(&api, "CartelInfluence.regionInfluence", seq),
            None => println!("regionInfluence carried no handle: {}", infl.result),
        }
        for region in 0..8 {
            let r = api.op(
                "invoke_method",
                json!({"handle": h, "method": "GetInfluence", "args": [region]}),
            );
            if r.ok {
                println!("GetInfluence({region}) = {}", r.result);
            } else {
                println!("GetInfluence({region}) failed: {:?}", r.error);
            }
        }
    }
}
