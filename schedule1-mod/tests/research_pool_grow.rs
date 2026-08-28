//! Research: grow the cartel goon pool past its 5 objects (the
//! war needs a real supply).
//!
//! Recipe under test (the proven world-object creation path):
//! 1. Clone a live CartelGoon via UnityEngine.Object.Instantiate.
//! 2. FishNet ServerManager.Spawn the clone (un-spawned clones
//!    get destroyed within seconds; proven with cash pickups).
//! 3. GoonPool.ReturnToPool(clone): the pool absorbs it.
//! 4. UnspawnedGoonCount must go UP, and a follow-up SpawnGoon
//!    at the player must produce a live goon from the grown
//!    pool.
//!
//! Run with the operator in-game and watching.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_pool_grow. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, first_handle, handle_of, ping_or_skip, player_position, walk};
use serde_json::json;

#[test]
fn grow_the_goon_pool() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some(pool) = first_handle(&api, "ScheduleOne.Cartel.GoonPool") else {
        return;
    };
    let count_before = api
        .op(
            "invoke_method",
            json!({"handle": pool, "method": "get_UnspawnedGoonCount", "args": []}),
        )
        .result
        .as_i64();
    println!("UnspawnedGoonCount before: {count_before:?}");

    // Clone an UNSPAWNED pooled goon (inactive template; cloning
    // a spawned one made FishNet's Spawn throw, tried 2026-08-08).
    let Some(goons) = walk(&api, "ScheduleOne.Cartel.CartelGoon") else {
        return;
    };
    let mut template = None;
    for g in &goons {
        let Some(h) = g["handle"].as_i64() else {
            continue;
        };
        if g["name"].as_str().unwrap_or("").ends_with("(Clone)") {
            continue;
        }
        let spawned = api
            .op(
                "invoke_method",
                json!({"handle": h, "method": "get_IsGoonSpawned", "args": []}),
            )
            .result
            .as_bool();
        if template.is_none() && spawned == Some(false) {
            template = Some(h);
        }
    }
    let Some(template) = template else {
        println!("every pooled goon is currently spawned; kill some garrisons first");
        return;
    };

    let clone = api.op(
        "invoke_static",
        json!({"class": "UnityEngine.Object", "method": "Instantiate",
               "args": [{"$handle": template}]}),
    );
    if handle_of(&clone.result).is_none() {
        println!("Instantiate failed: {:?} {}", clone.error, clone.result);
        return;
    }
    // Re-find as a properly-typed CartelGoon.
    let Some(after) = walk(&api, "ScheduleOne.Cartel.CartelGoon") else {
        return;
    };
    let Some(ch) = after
        .iter()
        .find(|g| g["name"].as_str().unwrap_or("").ends_with("(Clone)"))
        .and_then(|g| g["handle"].as_i64())
    else {
        println!("clone not found in post-instantiate walk");
        return;
    };
    println!("goon clone created");

    // Bypass the pool: the goon's OWN Spawn(position, appearance)
    // should do its full setup (it is a NetworkBehaviour with
    // Spawn RPCs). Appearance from the pool's randomizer.
    let Some((px, py, pz)) = player_position(&api) else {
        return;
    };
    let appearance = api.op(
        "invoke_method",
        json!({"handle": pool, "method": "GetRandomAppearance", "args": []}),
    );
    let Some(ah) = handle_of(&appearance.result) else {
        println!(
            "GetRandomAppearance carried no handle: {}",
            appearance.result
        );
        return;
    };
    // Activate first: the template is inactive, so the clone's
    // components never ran Awake (the NPCInventory null lists in
    // the 10:30 stack trace).
    let go = api.op(
        "invoke_method",
        json!({"handle": ch, "method": "get_gameObject", "args": []}),
    );
    if let Some(gh) = handle_of(&go.result) {
        let act = api.op(
            "invoke_method",
            json!({"handle": gh, "method": "SetActive", "args": [true]}),
        );
        println!("SetActive(true): ok={}", act.ok);
    }

    // FishNet-spawn the clone before the goon's own Spawn: its
    // ConfigureGoonSettings RPC needs an initialized
    // NetworkObject (10:31 stack trace).
    if let Some(gh) = handle_of(&go.result) {
        let sm = api.op(
            "invoke_static",
            json!({"class": "Il2CppFishNet.InstanceFinder", "method": "get_ServerManager", "args": []}),
        );
        if let Some(sh) = handle_of(&sm.result) {
            let net = api.op(
                "invoke_method",
                json!({"handle": sh, "method": "Spawn", "args": [{"$handle": gh}, null, {}]}),
            );
            println!("ServerManager.Spawn: ok={} {:?}", net.ok, net.error);
        }
    }

    // Signature per live probe errors: Spawn(GoonPool, Vector3).
    let spawn = api.op(
        "invoke_method",
        json!({"handle": ch, "method": "Spawn",
               "args": [{"$handle": pool}, {"x": px + 6.0, "y": py, "z": pz}]}),
    );
    println!(
        "clone.Spawn(pos, appearance): ok={} {:?}",
        spawn.ok, spawn.error
    );
    if spawn.ok {
        let live = api.op(
            "invoke_method",
            json!({"handle": ch, "method": "get_IsGoonSpawned", "args": []}),
        );
        println!("clone IsGoonSpawned: {}", live.result);
        println!(
            "OPERATOR CHECK: a SIXTH goon 6m from you, looking and behaving normal? Wait 30s: does it stay (not engine-destroyed)?"
        );
    }
}
