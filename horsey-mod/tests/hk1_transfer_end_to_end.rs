//! HK1 end-to-end: automated drive of the Home Location's drop-commit
//! vtable method on a real owned horse. Exercises the full transfer
//! codepath without requiring the operator to use the in-game cursor.
//!
//! For each target ("truck" / "pasture"):
//!   1. Optionally synthesize a calibration by writing fake cursor
//!      coords directly into the targets file via a new HTTP op.
//!   2. Read the BEFORE state of the horse (full bytes + container
//!      kind/sub).
//!   3. Invoke `hk1.transfer` via HTTP.
//!   4. Sleep 500ms for the game to render at least one frame.
//!   5. Read AFTER state and diff.
//!
//! Reports the vtable call's return value. Non-zero == drop accepted.
//! If even fake-calibrated coords cause a non-zero return AND the horse
//! state changes, the codepath is wired correctly; the remaining work
//! is just real calibration.

mod common;

use serde_json::{Value, json};
use std::time::Duration;

#[test]
fn transfer_end_to_end() {
    let Some(game) = common::launch("hk1_transfer_end_to_end") else {
        return;
    };
    let h = common::wait_for_target_horse(&game, Duration::from_secs(60));
    eprintln!("[target] {} at {}", h.name, h.ptr_s);

    // Use the read_cursor op to get whatever the cursor is now; treat
    // that as the "truck" calibration. Then nudge it +50 in X and save
    // as "pasture" so we have two distinct targets.
    let now = game
        .op_json("hk1.read_cursor", &json!({}))
        .unwrap_or(Value::Null);
    eprintln!("read_cursor -> {now}");
    let res = now.get("result").unwrap_or(&now);
    let cx = res.get("x").and_then(Value::as_f64).unwrap_or(0.0);
    let cy = res.get("y").and_then(Value::as_f64).unwrap_or(0.0);
    eprintln!(
        "synth calibration: truck=({cx},{cy})  pasture=({},{})",
        cx + 50.0,
        cy
    );

    let set_t = game
        .op_json(
            "hk1.set_target",
            &json!({
                "which": "truck", "x": cx, "y": cy,
            }),
        )
        .expect("set truck");
    eprintln!("set truck -> {set_t}");
    let set_p = game
        .op_json(
            "hk1.set_target",
            &json!({
                "which": "pasture", "x": cx + 50.0, "y": cy,
            }),
        )
        .expect("set pasture");
    eprintln!("set pasture -> {set_p}");

    for dest in ["truck", "pasture"] {
        eprintln!("\n=== transfer dest={dest} ===");
        let before = read_horse_u32(&game, h.id, 0x1d0);
        eprintln!("before +0x1d0 = {before:?}");
        let resp = game
            .op_json(
                "hk1.transfer",
                &json!({
                    "horse_ptr": h.id, "dest": dest,
                }),
            )
            .expect("transfer call");
        eprintln!("response: {resp}");
        std::thread::sleep(Duration::from_millis(500));
        let after = read_horse_u32(&game, h.id, 0x1d0);
        eprintln!("after  +0x1d0 = {after:?}");
        let result = resp
            .get("result")
            .and_then(|r| r.get("result"))
            .and_then(Value::as_u64);
        eprintln!("vtable call result = {result:?}  (non-zero = drop accepted)");
    }

    game.pass("transfer codepath exercised");
    // Keep the game alive so the operator can inspect what happened.
    std::process::exit(0);
}

fn read_horse_u32(game: &modforge::harness::RunningGame, horse_ptr: u64, off: u64) -> Option<u64> {
    let r = game
        .op_json(
            "mem.peek",
            &json!({
                "addr": horse_ptr + off, "kind": "u32",
            }),
        )
        .ok()?;
    r.get("result")
        .and_then(|x| x.get("value"))
        .and_then(Value::as_u64)
}
