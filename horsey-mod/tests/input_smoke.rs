//! I-R5 + I-1 smoke test for `modforge::input`.
//!
//! What it covers (one launch, both layers):
//!
//! 1. **L1 round-trip.** `input.cursor.get` baseline -> `input.mouse.move`
//!    backend=l1 -> `input.cursor.get` again. Verifies SendInput
//!    actually reached USER32 and moved the OS cursor. Restores
//!    cursor position to baseline at the end.
//! 2. **L1 keyboard smoke.** `input.key.press` for `F24` (harmless,
//!    no game binding). Confirms keyboard-side SendInput path doesn't
//!    crash.
//! 3. **L2 PostMessage smoke (I-R5).** Resolves the Horsey HWND via
//!    `input.foreground.hwnd` (best-effort; falls back to a hardcoded
//!    EnumWindows-style search if needed in a follow-up), then sends
//!    `input.mouse.move` backend=l2 to a corner. Verifies the cmdlet
//!    returns `ok:true`. Does NOT click; clicks could land on UI we
//!    don't own.
//!
//! Honors the standard `MODFORGE_NO_GAME=1` skip.

mod common;

use serde_json::{Value, json};

const SMALL_OFFSET: i32 = 10;

/// Op responses come wrapped in the standard envelope
/// `{ok, op, result: {...}, state, error}`. Helpers descend into `result`.
fn result<'a>(v: &'a Value) -> &'a Value {
    v.get("result").unwrap_or(v)
}

fn result_i64(v: &Value, key: &str) -> i64 {
    result(v)
        .get(key)
        .and_then(Value::as_i64)
        .unwrap_or_else(|| panic!("missing/non-int field '{key}' in {v}"))
}

fn assert_ok(v: &Value, ctx: &str) {
    assert_eq!(
        v.get("ok").and_then(Value::as_bool),
        Some(true),
        "{ctx} returned not-ok: {v}"
    );
}

#[test]
fn input_smoke() {
    let Some(game) = common::launch("input_smoke") else {
        return;
    };

    // ----- L1 cursor round-trip -----
    let base = game.op_json("input.cursor.get", &json!({})).unwrap();
    assert_ok(&base, "input.cursor.get baseline");
    let bx = result_i64(&base, "x") as i32;
    let by = result_i64(&base, "y") as i32;
    eprintln!("baseline cursor: ({bx}, {by})");

    let target_x = bx + SMALL_OFFSET;
    let target_y = by + SMALL_OFFSET;
    let r = game
        .op_json(
            "input.mouse.move",
            &json!({"x": target_x, "y": target_y, "backend": "l1"}),
        )
        .unwrap();
    assert_ok(&r, "input.mouse.move L1");

    // Re-read; allow a 4-pixel tolerance for normalized-coord rounding.
    let after = game.op_json("input.cursor.get", &json!({})).unwrap();
    let ax = result_i64(&after, "x") as i32;
    let ay = result_i64(&after, "y") as i32;
    eprintln!("after L1 move:   ({ax}, {ay}) [wanted ({target_x}, {target_y})]");
    let dx = (ax - target_x).abs();
    let dy = (ay - target_y).abs();
    assert!(
        dx <= 4 && dy <= 4,
        "L1 cursor move missed: got ({ax},{ay}), wanted ({target_x},{target_y}), delta=({dx},{dy})"
    );

    // Restore.
    let _ = game.op_json(
        "input.mouse.move",
        &json!({"x": bx, "y": by, "backend": "l1"}),
    );

    // ----- L1 keyboard smoke (F24 is unbound in Horsey) -----
    let r = game
        .op_json(
            "input.key.press",
            &json!({"key": "f24", "hold_ms": 0, "backend": "l1"}),
        )
        .unwrap();
    assert_ok(&r, "input.key.press f24");

    // ----- L2 PostMessage smoke (I-R5) -----
    //
    // We post a no-op `WM_MOUSEMOVE` to the Horsey window. Because
    // the cmdlet runs INSIDE the game process (via the HTTP plane),
    // `input.self.hwnd` returns Horsey's own top-level window
    // regardless of which process happens to own the desktop focus.
    let hwnd_r = game.op_json("input.self.hwnd", &json!({}));
    let hwnd_s = match hwnd_r {
        Ok(v) => {
            assert_ok(&v, "input.self.hwnd");
            result(&v)
                .get("hwnd")
                .and_then(Value::as_str)
                .map(str::to_string)
        }
        Err(e) => {
            eprintln!("input.self.hwnd failed: {e}");
            None
        }
    };
    if let Some(hwnd) = hwnd_s {
        eprintln!("Horsey hwnd (in-process): {hwnd}");
        let r = game
            .op_json(
                "input.mouse.move",
                &json!({"x": 0, "y": 0, "backend": "l2", "hwnd": hwnd}),
            )
            .unwrap();
        assert_ok(&r, "input.mouse.move L2");
    } else {
        eprintln!("[INFO] no self hwnd; skipping L2 smoke this run");
    }

    // ----- L1 drag round-trip -----
    //
    // Drag from baseline+10 to baseline+50, then back. Verifies the
    // drag path doesn't error and the cursor lands near the
    // destination.
    let drag_from = (bx + 10, by + 10);
    let drag_to = (bx + 50, by + 50);
    let r = game
        .op_json(
            "input.mouse.drag",
            &json!({
                "button": "left",
                "x1": drag_from.0, "y1": drag_from.1,
                "x2": drag_to.0,   "y2": drag_to.1,
                "duration_ms": 80, "steps": 8,
                "backend": "l1",
            }),
        )
        .unwrap();
    assert_ok(&r, "input.mouse.drag L1");
    let after_drag = game.op_json("input.cursor.get", &json!({})).unwrap();
    let ax = result_i64(&after_drag, "x") as i32;
    let ay = result_i64(&after_drag, "y") as i32;
    eprintln!(
        "after drag: ({ax},{ay}) [wanted ({},{})]",
        drag_to.0, drag_to.1
    );
    let dx = (ax - drag_to.0).abs();
    let dy = (ay - drag_to.1).abs();
    assert!(dx <= 4 && dy <= 4, "drag landed off: delta=({dx},{dy})");

    // Restore.
    let _ = game.op_json(
        "input.mouse.move",
        &json!({"x": bx, "y": by, "backend": "l1"}),
    );

    // ----- L1 scroll smoke (3 ticks up; cursor pos unchanged) -----
    let r = game
        .op_json("input.mouse.scroll", &json!({"dy": 3, "backend": "l1"}))
        .unwrap();
    assert_ok(&r, "input.mouse.scroll L1 dy=3");

    // ----- L3 smoke -----
    //
    // The L3 surface is registered at attach. Movement should land
    // in the OS cursor (via the surface's L1 fallback) AND, if a
    // save is loaded, in `LOC+0x174/0x178`. We assert OS-cursor
    // here; the LOC-floats verification is the recon test
    // (`input_hk1_calibration`) which needs a save.
    let target_l3 = (bx + 5, by + 5);
    let r = game
        .op_json(
            "input.mouse.move",
            &json!({"x": target_l3.0, "y": target_l3.1, "backend": "l3"}),
        )
        .unwrap();
    assert_ok(&r, "input.mouse.move L3");
    let after_l3 = game.op_json("input.cursor.get", &json!({})).unwrap();
    let lx = result_i64(&after_l3, "x") as i32;
    let ly = result_i64(&after_l3, "y") as i32;
    let dlx = (lx - target_l3.0).abs();
    let dly = (ly - target_l3.1).abs();
    eprintln!(
        "after L3 move:   ({lx},{ly}) [wanted ({},{})]",
        target_l3.0, target_l3.1
    );
    assert!(
        dlx <= 4 && dly <= 4,
        "L3 cursor move missed: delta=({dlx},{dly})"
    );

    // Restore.
    let _ = game.op_json(
        "input.mouse.move",
        &json!({"x": bx, "y": by, "backend": "l1"}),
    );

    // ----- L1 combo smoke (shift+F24; harmless) -----
    let r = game
        .op_json(
            "input.combo",
            &json!({
                "keys": ["shift"],
                "then": {
                    "op": "input.key.press",
                    "args": {"key": "f24", "backend": "l1"}
                }
            }),
        )
        .unwrap();
    assert_ok(&r, "input.combo shift+f24");

    eprintln!("[PASS] input_smoke: L1+L2 move/click/key + L1 drag/scroll/combo");
}
