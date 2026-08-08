//! Research question 1 (docs/research.md): what class owns the
//! town's map regions and what state it carries.
//!
//! ANSWERED 2026-08-07, see docs/research.md. This test remains
//! the living proof: it dumps the full region table and the live
//! cartel influence per region via generic handle chaining.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_map. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running so
//! the workspace suite stays green.

mod common;
use common::{api, dump_sequence, first_handle, handle_of, ping_or_skip};
use serde_json::json;

#[test]
fn map_region_owner() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

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
