//! I-2d recon: capture the mapping between OS screen pixels (what
//! `modforge::input` sends) and the Home Location's cursor floats
//! (`LOC+0x174`/`+0x178`, what the HK1 click handler watches).
//!
//! Strategy: bracket the screen with 4 + center moves, read the LOC
//! floats at each stop, report deltas. If LOC delta tracks screen
//! delta linearly, we have a usable affine transform for HK1
//! synthetic-input migration. If not, we need a richer L3
//! `InputSurface` impl that writes the LOC floats directly.
//!
//! Skips cleanly when no game is running (`MODFORGE_NO_GAME=1`).
//! Requires the user (or test runner) to have a save loaded so that
//! the Home Location exists; `hk1.read_cursor` returns `null` when
//! it's unset (e.g. at the main menu).

mod common;

use serde_json::{Value, json};

fn result<'a>(v: &'a Value) -> &'a Value {
    v.get("result").unwrap_or(v)
}

fn cursor_floats(game: &modforge::harness::RunningGame) -> Option<(f32, f32)> {
    let v = game.op_json("hk1.read_cursor", &json!({})).ok()?;
    let r = result(&v);
    let x = r.get("x").and_then(Value::as_f64)? as f32;
    let y = r.get("y").and_then(Value::as_f64)? as f32;
    Some((x, y))
}

fn screen_cursor(game: &modforge::harness::RunningGame) -> (i32, i32) {
    let v = game.op_json("input.cursor.get", &json!({})).unwrap();
    let r = result(&v);
    (
        r.get("x").and_then(Value::as_i64).unwrap() as i32,
        r.get("y").and_then(Value::as_i64).unwrap() as i32,
    )
}

fn screen_move(game: &modforge::harness::RunningGame, x: i32, y: i32) {
    let _ = game
        .op_json(
            "input.mouse.move",
            &json!({"x": x, "y": y, "backend": "l1"}),
        )
        .unwrap();
    // Small settle so the engine's per-frame input pump can pick the
    // new OS cursor up before we read LOC.
    std::thread::sleep(std::time::Duration::from_millis(50));
}

#[test]
fn hk1_calibration_recon() {
    let Some(game) = common::launch("hk1_calibration_recon") else {
        return;
    };

    let (bx, by) = screen_cursor(&game);
    eprintln!("baseline screen cursor: ({bx}, {by})");

    // Sample a 5-point plus pattern around baseline, +/- 100px.
    let samples = [
        (bx, by),
        (bx + 100, by),
        (bx - 100, by),
        (bx, by + 100),
        (bx, by - 100),
    ];

    let mut table: Vec<(i32, i32, Option<(f32, f32)>)> = Vec::new();
    for (sx, sy) in samples {
        screen_move(&game, sx, sy);
        let loc = cursor_floats(&game);
        eprintln!("screen=({sx},{sy}) -> LOC={loc:?}");
        table.push((sx, sy, loc));
    }

    // Restore.
    screen_move(&game, bx, by);

    // Report deltas relative to the first sample (baseline).
    let (base_sx, base_sy, base_loc) = table[0];
    if let Some((base_lx, base_ly)) = base_loc {
        eprintln!("--- deltas vs baseline ---");
        for (sx, sy, loc) in &table[1..] {
            let dsx = sx - base_sx;
            let dsy = sy - base_sy;
            if let Some((lx, ly)) = loc {
                let dlx = lx - base_lx;
                let dly = ly - base_ly;
                let kx = if dsx != 0 { dlx / dsx as f32 } else { 0.0 };
                let ky = if dsy != 0 { dly / dsy as f32 } else { 0.0 };
                eprintln!(
                    "  d_screen=({dsx},{dsy}) d_loc=({dlx:+.2},{dly:+.2}) ratio=({kx:.4},{ky:.4})"
                );
            } else {
                eprintln!("  d_screen=({dsx},{dsy}) d_loc=<unreadable>");
            }
        }
    } else {
        eprintln!(
            "[INFO] LOC cursor unreadable at baseline; \
likely no save loaded. Re-run with a save active."
        );
    }

    eprintln!("[RECON] hk1_calibration_recon complete; see deltas above");
}
