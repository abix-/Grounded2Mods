//! HK1 transfer end-to-end via HTTP, attach-mode. With the new
//! `modforge::seh::guard` wrap on the vtable call, this can be invoked
//! safely as a test: a crash inside vanilla logs a SehError instead of
//! killing the game, so we get the diagnostic without losing the
//! session.
//!
//! Runs:
//!   1. attach to live game
//!   2. find target horse
//!   3. read +0x1d0 / +0x1dc baseline
//!   4. call hk1.transfer dest=truck
//!   5. read +0x1d0 / +0x1dc after
//!   6. report
//!
//! Calibration must already be saved at <dll_dir>/hk1_targets.json.
//! Designed for MODFORGE_ATTACH=1.

mod common;

use serde_json::{Value, json};
use std::time::Duration;

#[test]
fn transfer_seh_iteration() {
    let Some(game) = common::launch("hk1_transfer_seh_iteration") else {
        return;
    };
    let h = common::wait_for_target_horse(&game, Duration::from_secs(30));
    eprintln!("\n[target] {} at {}", h.name, h.ptr_s);

    let dest = std::env::var("HK1_DEST").unwrap_or_else(|_| "truck".to_string());
    eprintln!("[dest] {dest}");

    // Ensure both targets are set on the live mod (the JSON-on-disk
    // cache might be stale from a previous launch).
    let _ = game
        .op_json(
            "hk1.set_target",
            &json!({
                "which": "truck", "x": 13.263803f64, "y": 8.902644f64,
            }),
        )
        .expect("set truck");
    let _ = game
        .op_json(
            "hk1.set_target",
            &json!({
                "which": "pasture", "x": 3.407552f64, "y": 3.0829327f64,
            }),
        )
        .expect("set pasture");

    let r_before_kind = read_u32(&game, h.id + 0x1d0);
    let r_before_sub = read_u32(&game, h.id + 0x1dc);
    eprintln!("BEFORE  horse+0x1d0={r_before_kind:?}  horse+0x1dc={r_before_sub:?}");

    let resp = game.op_json(
        "hk1.transfer",
        &json!({
            "horse_ptr": h.id, "dest": dest,
        }),
    );
    eprintln!("\ntransfer response: {resp:?}");

    // 500ms for at least one game frame.
    std::thread::sleep(Duration::from_millis(500));

    let r_after_kind = read_u32(&game, h.id + 0x1d0);
    let r_after_sub = read_u32(&game, h.id + 0x1dc);
    eprintln!("AFTER   horse+0x1d0={r_after_kind:?}  horse+0x1dc={r_after_sub:?}");

    // Whatever happens, the game should still be responsive. Ping again
    // by reading owned horses; failure here means the SEH guard didn't
    // protect us.
    let still_alive = game.op_json("gamestate.owned_horses", &json!({})).is_ok();
    eprintln!("game still responsive after transfer: {still_alive}");

    game.pass(&format!(
        "transfer dest={dest} resp_ok={} still_alive={still_alive}",
        resp.is_ok()
    ));
}

fn read_u32(game: &modforge::harness::RunningGame, addr: u64) -> Option<u64> {
    let r = game
        .op_json("mem.peek", &json!({"addr": addr, "kind": "u32"}))
        .ok()?;
    r.get("result")
        .and_then(|x| x.get("value"))
        .and_then(Value::as_u64)
}
