//! Research: usable spawn anchor positions INSIDE each map
//! region, for zone-stationed mobs (the operator's conquest
//! model: mobs hold regions, the player goes to them).
//!
//! Candidates per MapRegionData (proven fields, research.md 1):
//! RegionDeliveryLocations (things with transforms in-region)
//! and RegionBounds: PolygonalZone (its vertex list, if
//! reachable, gives the region's shape).
//!
//! ```text
//! cargo test -p schedule1-mod --test research_zones. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, count_of, first_handle, handle_of, parse_vec3, ping_or_skip};
use serde_json::json;

#[test]
fn region_spawn_anchors() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some(map) = first_handle(&api, "ScheduleOne.Map.Map") else {
        return;
    };
    let regions = api.op("read_field", json!({"handle": map, "field": "Regions"}));
    let Some(rh) = handle_of(&regions.result) else {
        println!("Regions carried no handle: {}", regions.result);
        return;
    };
    let Some(n) = count_of(&api, rh) else {
        println!("Regions count unavailable");
        return;
    };
    for i in 0..n {
        let item = api.op("invoke_method", json!({"handle": rh, "method": "get_Item", "args": [i]}));
        let Some(reg) = handle_of(&item.result) else { continue };
        let name = api.op("read_field", json!({"handle": reg, "field": "Name"}));
        println!("== region {}: {}", i, name.result);

        // Delivery locations: positions inside the region?
        let dl = api.op("read_field", json!({"handle": reg, "field": "RegionDeliveryLocations"}));
        if let Some(dlh) = handle_of(&dl.result) {
            let count = count_of(&api, dlh).unwrap_or(0);
            println!("   {count} delivery location(s)");
            for j in 0..count.min(3) {
                let e = api.op("invoke_method", json!({"handle": dlh, "method": "get_Item", "args": [j]}));
                if let Some(eh) = handle_of(&e.result) {
                    println!("   [{}] type={}", j, e.result["il2cpp_type"]);
                    let t = api.op("read_field", json!({"handle": eh, "field": "transform"}));
                    if let Some(th) = handle_of(&t.result) {
                        let p = api.op("invoke_method", json!({"handle": th, "method": "get_position", "args": []}));
                        println!("       pos={:?}", parse_vec3(&p.result));
                        api.op("release_handle", json!({"handle": th}));
                    }
                    api.op("release_handle", json!({"handle": eh}));
                }
            }
            api.op("release_handle", json!({"handle": dlh}));
        }

        // The bounds polygon: vertices reachable?
        let rb = api.op("read_field", json!({"handle": reg, "field": "RegionBounds"}));
        if let Some(rbh) = handle_of(&rb.result) {
            for field in ["Points", "points", "Vertices", "LocalPoints"] {
                let p = api.op("read_field", json!({"handle": rbh, "field": field}));
                if p.ok {
                    println!("   RegionBounds.{field}: {}", p.result);
                    if let Some(ph) = handle_of(&p.result) {
                        println!("     count={:?}", count_of(&api, ph));
                    }
                    break;
                }
            }
            api.op("release_handle", json!({"handle": rbh}));
        }
        api.op("release_handle", json!({"handle": reg}));
    }
}
