//! Research: enumerate FishNet's registered spawnable prefabs
//! (NetworkManager.SpawnablePrefabs). S1API's custom-NPC recipe
//! clones from THESE (real prefabs, not live scene objects),
//! which is why its spawns initialize cleanly. The names tell us
//! what we can mint: goons, police, civilians, employees.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_prefabs. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, handle_of, ping_or_skip};
use serde_json::json;

#[test]
fn list_spawnable_prefabs() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let nm = api.op(
        "invoke_static",
        json!({"class": "Il2CppFishNet.InstanceFinder", "method": "get_NetworkManager", "args": []}),
    );
    let Some(nmh) = handle_of(&nm.result) else {
        println!("no NetworkManager: {}", nm.result);
        return;
    };
    let sp = api.op("invoke_method", json!({"handle": nmh, "method": "get_SpawnablePrefabs", "args": []}));
    let Some(sph) = handle_of(&sp.result) else {
        println!("no SpawnablePrefabs: {}", sp.result);
        return;
    };
    let count = api
        .op("invoke_method", json!({"handle": sph, "method": "GetObjectCount", "args": []}))
        .result
        .as_i64()
        .unwrap_or(0);
    println!("{count} spawnable prefab(s):");
    for i in 0..count {
        let o = api.op("invoke_method", json!({"handle": sph, "method": "GetObject", "args": [true, i]}));
        let Some(oh) = handle_of(&o.result) else {
            println!("  [{i}] <no handle>");
            continue;
        };
        let name = api.op("invoke_method", json!({"handle": oh, "method": "get_name", "args": []}));
        println!("  [{i}] {}", name.result);
        api.op("release_handle", json!({"handle": oh}));
    }
}
