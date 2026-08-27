//! The game's own asset index: every piece it ships, loaded or
//! not, and pulling an unloaded one into memory (src/assets.rs).
//!
//! Walking GObjects only sees what is in memory, which varies by
//! area. This asks Unreal's registry instead.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_assets -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use serde_json::json;

/// How many static meshes does the game actually ship, versus
/// how many happen to be loaded?
#[test]
fn registry_sees_more_than_memory() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let r = api.op("asset_inventory", json!({"class": "StaticMesh"}));
    assert!(r.ok, "asset_inventory failed: {:?}", r.error);
    let total = r.result["total"].as_u64().unwrap_or(0);
    println!("registry reports {total} static mesh(es) in the game");

    let loaded = api.op("mesh_info", json!({"prefix": ""}));
    let loaded_n = loaded.result["count"].as_u64().unwrap_or(0);
    println!("{loaded_n} of them are loaded right now");

    assert!(total > 0, "registry returned nothing; is it populated in shipping?");
}

/// Every wall the game ships, whether or not this area uses one.
#[test]
fn every_wall_in_the_game() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let r = api.op(
        "asset_inventory",
        json!({"class": "StaticMesh", "contains": "SM_Wall"}),
    );
    assert!(r.ok, "asset_inventory failed: {:?}", r.error);
    let assets = r.result["assets"].as_array().cloned().unwrap_or_default();
    println!("{} wall piece(s) in the game:", assets.len());
    let mut names: Vec<&str> = assets
        .iter()
        .filter_map(|a| a["name"].as_str())
        .collect();
    names.sort_unstable();
    for n in &names {
        println!("  {n}");
    }
}

/// Pull a piece into memory that is not currently loaded, which
/// is what lets generation use ANY piece rather than only what
/// the current area happens to have.
#[test]
#[ignore = "loads assets into the live game"]
fn load_an_unloaded_piece() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    // What is loaded right now?
    let loaded = api.op("mesh_info", json!({"prefix": "SM_Wall"}));
    let loaded_names: Vec<String> = loaded.result["meshes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|m| m["name"].as_str().map(str::to_string))
        .collect();
    println!("{} wall piece(s) loaded now", loaded_names.len());

    // What does the game have that is NOT loaded?
    let all = api.op(
        "asset_inventory",
        json!({"class": "StaticMesh", "contains": "SM_Wall_"}),
    );
    let candidates = all.result["assets"].as_array().cloned().unwrap_or_default();
    let Some(target) = candidates
        .iter()
        .find(|a| {
            a["name"]
                .as_str()
                .map(|n| !loaded_names.iter().any(|l| l == n))
                .unwrap_or(false)
        })
        .cloned()
    else {
        println!("SKIP: every wall the registry knows is already loaded");
        return;
    };

    let name = target["name"].as_str().unwrap_or("?").to_string();
    println!("loading {name}, which is not in memory");
    let r = api.op(
        "load_asset",
        json!({
            "package_fname": target["package_fname"],
            "asset_fname": target["asset_fname"],
        }),
    );
    assert!(r.ok, "load_asset failed: {:?}", r.error);
    println!("load result: {}", r.result);
    assert_eq!(r.result["loaded"], json!(true), "asset did not load");

    // It should now be visible to a plain memory walk.
    let after = api.op("mesh_info", json!({"prefix": &name}));
    let found = after.result["meshes"]
        .as_array()
        .map(|a| a.iter().any(|m| m["name"].as_str() == Some(name.as_str())))
        .unwrap_or(false);
    assert!(found, "{name} loaded but is not visible in memory");
    println!("{name} is now in memory and usable");
}

/// What does the registry carry per asset, beyond the name?
///
/// `FAssetData` is 0x68 bytes and only its first 0x28 are named
/// in the dump we have. The rest should hold `TagsAndValues`, the
/// searchable metadata Unreal cooks in. If a static mesh's bounds
/// are in there, the parts list needs no loading at all: 1,500
/// blocking loads become one registry query (pieces.md).
///
/// Read-only.
#[test]
fn what_the_registry_carries_per_asset() {
    let Some(api) = api_or_skip() else { return };
    let r = api.op("asset_data_bytes", serde_json::json!({ "class": "StaticMesh", "count": 4 }));
    if !r.ok {
        println!("asset_data_bytes failed: {:?}", r.error);
        return;
    }
    println!("FAssetData stride {}", r.result["stride"]);
    for a in r.result["assets"].as_array().cloned().unwrap_or_default() {
        println!("
{}", a["name"].as_str().unwrap_or("?"));
        let hex = a["bytes"].as_str().unwrap_or("");
        // Sixteen bytes a line, with the offset, so fields line up
        // against the known layout.
        for (i, chunk) in hex.split(' ').collect::<Vec<_>>().chunks(16).enumerate() {
            println!("  +{:#04x}  {}", i * 16, chunk.join(" "));
        }
    }
}

/// Does Unreal expose the tag map as a FUNCTION we can call?
///
/// `FAssetData` carries `TagsAndValues` as a pointer at +0x38,
/// and decoding a shared TMap out of raw memory is real work.
/// `AssetRegistryHelpers` is a Blueprint library, so if it has a
/// tag getter we can call it through ProcessEvent exactly the way
/// `GetAssetsByClass` is called, and skip the decoding entirely.
///
/// Read-only.
#[test]
fn does_the_registry_expose_a_tag_getter() {
    let Some(api) = api_or_skip() else { return };
    for class in ["AssetRegistryHelpers", "AssetRegistry"] {
        // By NAME: these are static Blueprint libraries with no
        // live instance, so `class_functions` cannot see them.
        let r = api.op("class_functions_by_name", serde_json::json!({ "class": class }));
        println!("
=== {class}");
        if !r.ok {
            println!("  failed: {:?}", r.error);
            continue;
        }
        for f in r.result["functions"].as_array().cloned().unwrap_or_default() {
            let name = f["name"].as_str().unwrap_or("?");
            let mark = if name.to_lowercase().contains("tag") { ">>" } else { "  " };
            println!("{mark} {name:<52} parms={} bytes={}", f["num_parms"], f["parms_size"]);
        }
    }
}
