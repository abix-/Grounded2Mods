//! End-to-end smoke for `game.exit_to_overworld` with FULL DIAGNOSTIC LOGGING.
//!
//! When this test fails, the eprintln! stream should make the cause obvious
//! without re-running. Captures: game window handle + bounds, cursor before
//! and after click, scene_id transitions with timestamps, op responses.

mod common;

use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn op(game: &modforge::harness::RunningGame, name: &str, args: Value) -> Value {
    match game.op_json(name, &args) {
        Ok(v) => {
            eprintln!("[op] {name} {args} -> {v}");
            v
        }
        Err(e) => {
            eprintln!("[op] {name} {args} -> ERR {e}");
            Value::Null
        }
    }
}

fn read_scene_id(game: &modforge::harness::RunningGame) -> Option<i32> {
    let v = game.op_json("hk1.probe.active_location", &json!({})).ok()?;
    let r = v.get("result").unwrap_or(&v);
    r.get("active_scene_id")
        .and_then(Value::as_i64)
        .map(|n| n as i32)
}

#[test]
fn exit_to_overworld_smoke() {
    let Some(game) = common::launch("exit_to_overworld_smoke") else {
        return;
    };

    eprintln!("\n=== diag: pre-test state ===");
    op(&game, "ping", json!({}));
    op(&game, "input.self.hwnd", json!({}));
    op(&game, "input.foreground.hwnd", json!({}));
    op(&game, "input.cursor.get", json!({}));

    eprintln!("\n=== step 1: wait for save load (any owned horse) ===");
    let h = common::wait_for_target_horse(&game, Duration::from_secs(120));
    eprintln!("[ready] horse '{}' at {} (id {})", h.name, h.ptr_s, h.id);

    let baseline = read_scene_id(&game);
    eprintln!("[baseline] active_scene_id = {baseline:?} (should be -1 = overworld)");
    assert_eq!(baseline, Some(-1), "save loaded but not on overworld");

    eprintln!("\n=== step 2: synthetic click on house door ===");
    let (hx, hy) = (983i32, 394i32);
    eprintln!("[plan] target click @ ({hx}, {hy}) backend=l1");

    // Move first so we can confirm where the OS thinks the cursor is.
    op(
        &game,
        "input.mouse.move",
        json!({"x": hx, "y": hy, "backend": "l1"}),
    );
    op(&game, "input.cursor.get", json!({}));
    op(&game, "input.foreground.hwnd", json!({}));

    let click = op(
        &game,
        "input.mouse.click",
        json!({
            "button": "left", "x": hx, "y": hy, "backend": "l1"
        }),
    );
    eprintln!("[click] response={click}");
    op(&game, "input.cursor.get", json!({}));

    eprintln!("\n=== step 3: poll scene_id (max 30s) ===");
    let t0 = Instant::now();
    let mut last_id = baseline;
    let deadline = t0 + Duration::from_secs(30);
    while Instant::now() < deadline {
        let now_id = read_scene_id(&game);
        if now_id != last_id {
            eprintln!(
                "[poll t={:?}] scene_id changed {last_id:?} -> {now_id:?}",
                t0.elapsed()
            );
            last_id = now_id;
            if now_id.is_some() && now_id != Some(-1) {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    eprintln!(
        "[poll done] final scene_id = {last_id:?} (elapsed {:?})",
        t0.elapsed()
    );

    assert!(
        matches!(last_id, Some(id) if id != -1),
        "click failed to enter a scene -- scene_id stayed at {baseline:?}. \
         Click likely missed the house door, the game didn't have focus, or the \
         coords were stale. Check the [op] log above for the cursor's actual \
         position after the click."
    );

    eprintln!("\n=== step 4: call game.exit_to_overworld ===");
    let pre_exit_id = read_scene_id(&game);
    let r = op(&game, "game.exit_to_overworld", json!({}));
    let inner = r.get("result").unwrap_or(&r);
    assert_eq!(
        inner.get("ok").and_then(|v| v.as_bool()),
        Some(true),
        "op did not return ok=true: {r}"
    );

    eprintln!("\n=== step 5: poll scene_id back to -1 (max 5s) ===");
    let t0 = Instant::now();
    let mut now_id = read_scene_id(&game);
    while now_id != Some(-1) && t0.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(100));
        now_id = read_scene_id(&game);
    }
    eprintln!(
        "[poll] scene_id after exit = {now_id:?} (elapsed {:?})",
        t0.elapsed()
    );
    assert_eq!(
        now_id,
        Some(-1),
        "active_scene_id did not return to -1 after game.exit_to_overworld; \
         was {pre_exit_id:?} before, stuck at {now_id:?} after",
    );

    eprintln!("\n=== step 6: second call should report skipped ===");
    let r2 = op(&game, "game.exit_to_overworld", json!({}));
    let inner2 = r2.get("result").unwrap_or(&r2);
    assert_eq!(
        inner2.get("ok").and_then(|v| v.as_bool()),
        Some(true),
        "second call did not return ok=true: {r2}"
    );
    assert_eq!(
        inner2.get("skipped").and_then(|v| v.as_bool()),
        Some(true),
        "second call did not report skipped=true: {r2}"
    );

    eprintln!("\n=== PASS ===");
}
