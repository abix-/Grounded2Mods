//! Generated rooms from the game's modular kit (src/rooms.rs).
//!
//! `room_plan` returns what a room WOULD be built from, so the
//! binder is checked without spawning; `build_room` puts one in
//! the world.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_rooms -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use serde_json::json;

/// An 8x8 room at 4 m modules: 2 segments per side, a door in one
/// of them, windows in three others, four floor tiles.
#[test]
fn room_plan_uses_the_kit_correctly() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let r = api.op(
        "room_plan",
        json!({"width": 8.0, "length": 8.0, "height": 3.0}),
    );
    assert!(r.ok, "room_plan failed: {:?}", r.error);
    let pieces = r.result["pieces"].as_array().cloned().unwrap_or_default();
    println!("{} piece(s):", pieces.len());
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for p in &pieces {
        let mesh = p["mesh"].as_str().unwrap_or("?").to_string();
        *counts.entry(mesh).or_default() += 1;
    }
    for (mesh, n) in &counts {
        println!("  {n:>3} x {mesh}");
    }

    let n = |m: &str| *counts.get(m).unwrap_or(&0);
    assert_eq!(n("SM_Floor_400x400"), 4, "an 8x8 floor is four 4 m tiles");
    assert_eq!(n("SM_WallDoor_400x300"), 1, "exactly one door segment");
    assert_eq!(n("SM_WallWindow_400x300"), 3, "one window per other side");
    assert_eq!(
        n("SM_Wall_400x300"),
        4,
        "the remaining segments are plain walls"
    );
    // Every mesh the binder chose must be a real kit part.
    for mesh in counts.keys() {
        let probe = api.op("mesh_info", json!({"prefix": mesh}));
        assert!(probe.ok, "mesh_info failed for {mesh}");
        let found = probe.result["meshes"]
            .as_array()
            .map(|a| a.iter().any(|m| m["name"].as_str() == Some(mesh.as_str())))
            .unwrap_or(false);
        assert!(found, "{mesh} is not a loaded mesh");
    }
    println!("every chosen mesh exists in the loaded kit");
}

/// Odd sizes fall back through the module list: a 7 m wall becomes
/// 4 + 2 + 1.
#[test]
fn odd_sizes_fill_with_smaller_modules() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let r = api.op(
        "room_plan",
        json!({"width": 7.0, "length": 4.0, "height": 3.0, "windows": false}),
    );
    assert!(r.ok, "room_plan failed: {:?}", r.error);
    let pieces = r.result["pieces"].as_array().cloned().unwrap_or_default();
    let mut names: Vec<&str> = pieces.iter().filter_map(|p| p["mesh"].as_str()).collect();
    names.sort_unstable();
    names.dedup();
    println!("meshes used: {names:?}");
    assert!(
        names.iter().any(|m| m.contains("200x300")),
        "a 7 m wall should use a 2 m module: {names:?}"
    );
    assert!(
        names.iter().any(|m| m.contains("100x300")),
        "a 7 m wall should use a 1 m module: {names:?}"
    );
}

/// Build one in the world, in front of the player.
#[test]
#[ignore = "spawns actors in the live game"]
fn build_a_room() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let r = api.op(
        "build_room",
        json!({"width": 8.0, "length": 8.0, "height": 3.0, "away": 1200.0}),
    );
    assert!(r.ok, "build_room failed: {:?}", r.error);
    println!("built: {}", r.result);
    assert!(
        r.result["placed"].as_u64().unwrap_or(0) >= 12,
        "expected a full shell, got {}",
        r.result
    );
}
