//! HK1 move-verify (operator-driven): confirm the positional detector
//! tracks a real trailer<->pasture move. Launches fresh, polls
//! `gamestate.owned_horses` every second for ~90s, prints when any
//! horse's container label or scene position changes. Operator enters
//! the house and drags a horse trailer<->pasture during the window.
//!
//! The key thing it answers: when a horse moves to the pasture, does its
//! `+0x1d4/+0x1d8` drop to (0,0). So the current "non-zero = trailer"
//! rule holds. Or land on a real pasture coordinate (~3,3), in which
//! case the classifier needs a region split. #[ignore]d; run with
//! `-- --ignored`.

mod common;

use serde_json::{json, Value};

fn snapshot(game: &modforge::harness::RunningGame) -> Vec<(u64, String, f32, f32)> {
    let v = game.op_json("gamestate.owned_horses", &json!({})).unwrap_or(Value::Null);
    let r = v.get("result").unwrap_or(&v);
    r.get("horses")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|h| {
            let idx = h.get("idx").and_then(Value::as_u64).unwrap_or(0);
            let c = h.get("container").and_then(Value::as_str).unwrap_or("?").to_string();
            let x = h.get("scene_x").and_then(Value::as_f64).unwrap_or(f64::NAN) as f32;
            let y = h.get("scene_y").and_then(Value::as_f64).unwrap_or(f64::NAN) as f32;
            (idx, c, x, y)
        })
        .collect()
}

#[test]
#[ignore = "manual: operator moves a horse during the 90s window"]
fn hk1_move_verify() {
    let Some(game) = common::launch("hk1_container_watch") else { return };

    // Wait out the load race.
    let dl = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while snapshot(&game).is_empty() && std::time::Instant::now() < dl {
        std::thread::sleep(std::time::Duration::from_millis(250));
    }

    let mut last = snapshot(&game);
    eprintln!("[VERIFY] baseline:");
    for (idx, c, x, y) in &last {
        eprintln!("  horse[{idx}] container={c} scene=({x}, {y})");
    }
    eprintln!("[VERIFY] >>> NOW: enter your house, then drag a horse between trailer and pasture. Watching 90s...");

    let end = std::time::Instant::now() + std::time::Duration::from_secs(90);
    while std::time::Instant::now() < end {
        std::thread::sleep(std::time::Duration::from_millis(1000));
        let now = snapshot(&game);
        for (i, (idx, c, x, y)) in now.iter().enumerate() {
            if let Some((_, pc, px, py)) = last.get(i) {
                if c != pc || (x - px).abs() > 0.01 || (y - py).abs() > 0.01 {
                    eprintln!("[CHANGE] horse[{idx}] container {pc}->{c}  scene ({px},{py})->({x},{y})");
                }
            }
        }
        last = now;
    }

    eprintln!("[VERIFY] done. final:");
    for (idx, c, x, y) in &last {
        eprintln!("  horse[{idx}] container={c} scene=({x}, {y})");
    }
}
