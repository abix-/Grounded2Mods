//! A1 (HK1 prereq): capture the screen coords of the house door
//! while the player hovers it on the world map.
//!
//! Attaches to an already-running game (run with `MODFORGE_ATTACH=1`)
//! so the user can position the cursor manually first. Reads
//! `input.cursor.get`, validates the result, and persists to
//! `<dll_dir>/menu_targets.json` keyed `home_door_from_truck_spawn`.
//!
//! Usage:
//!   1. Launch the game via `horsey-play` (truck spawns on world map).
//!   2. Hover the cursor over the house door (do NOT click).
//!   3. `MODFORGE_ATTACH=1 cargo test -p horsey-mod \
//!        --test hk1_capture_house_coords. --nocapture`
//!
//! Re-runnable: each call overwrites the `home_door_from_truck_spawn`
//! entry, leaving other keys in `menu_targets.json` untouched.

mod common;

use serde_json::{Value, json};
use std::path::PathBuf;

const TARGETS_FILENAME: &str = "menu_targets.json";
const KEY: &str = "home_door_from_truck_spawn";

fn dll_dir() -> PathBuf {
    // Same layout as the release/debug horsey.dll lives in.
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    manifest
        .parent()
        .expect("workspace root")
        .join("target")
        .join("x86_64-pc-windows-msvc")
        .join(profile)
}

#[test]
fn capture_house_door_coords() {
    let Some(game) = common::launch("hk1_capture_house_coords") else {
        return;
    };

    let v = game
        .op_json("input.cursor.get", &json!({}))
        .expect("input.cursor.get failed");
    let r = v.get("result").unwrap_or(&v);
    let x = r
        .get("x")
        .and_then(Value::as_i64)
        .expect("cursor.x missing");
    let y = r
        .get("y")
        .and_then(Value::as_i64)
        .expect("cursor.y missing");
    eprintln!("[CAPTURE] cursor at screen ({x}, {y})");

    let dir = dll_dir();
    std::fs::create_dir_all(&dir).expect("mkdir target dir");
    let path = dir.join(TARGETS_FILENAME);

    let mut doc: Value = if path.exists() {
        let s = std::fs::read_to_string(&path).expect("read existing targets");
        serde_json::from_str(&s).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };
    let obj = doc.as_object_mut().expect("targets json must be an object");
    obj.insert(KEY.to_string(), json!({"x": x, "y": y}));

    let pretty = serde_json::to_string_pretty(&doc).expect("serialize targets");
    std::fs::write(&path, pretty).expect("write targets");
    eprintln!(
        "[CAPTURE] wrote {} = {{x:{x}, y:{y}}} to {}",
        KEY,
        path.display()
    );
}
