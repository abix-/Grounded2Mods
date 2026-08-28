//! HK1 slice 1. Detection gate (positional model, live-confirmed).
//!
//! Trailer vs pasture is decided by the horse's scene placement at
//! +0x1d4/+0x1d8 (live-confirmed 2026-06-23): a trailer horse reads its
//! trailer position (~13, 9), a pasture horse reads (0, 0) on the
//! overworld. `gamestate.owned_horses` classifies each owned horse via
//! that field. This gate asserts the op returns a `container` of trailer
//! or pasture for every owned horse, and prints the scene positions.

mod common;

use serde_json::{Value, json};

#[test]
fn owned_horses_report_container() {
    let Some(game) = common::launch("horse_container_detect") else {
        return;
    };

    // Wait out the save-load race.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut v = game
        .op_json("gamestate.owned_horses", &json!({}))
        .expect("owned_horses op");
    loop {
        let n = v
            .get("result")
            .unwrap_or(&v)
            .get("count")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if n > 0 || std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
        v = game
            .op_json("gamestate.owned_horses", &json!({}))
            .expect("owned_horses op");
    }

    let r = v.get("result").unwrap_or(&v);
    let horses = r
        .get("horses")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    eprintln!("[OWNED] {} horse(s)", horses.len());
    assert!(
        !horses.is_empty(),
        "no owned horses -- gamestate not loaded?"
    );

    for h in &horses {
        let idx = h.get("idx").and_then(Value::as_u64).unwrap_or(0);
        let sx = h.get("scene_x");
        let sy = h.get("scene_y");
        let kind = h.get("container").and_then(Value::as_str);
        eprintln!("  horse[{idx}] scene=({sx:?}, {sy:?}) container={kind:?}");
        let kind = kind.expect("missing `container` classification string");
        assert!(
            matches!(kind, "trailer" | "pasture"),
            "horse[{idx}] unexpected container: {kind}"
        );
    }
}
