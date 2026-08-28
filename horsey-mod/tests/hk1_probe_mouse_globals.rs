//! HK1 Stage S0 probe: read mouse_screen_x/y at HLT's documented RVAs.
//!
//! HLT (`research/prior-art/HorseyLiveTweaks/src/core/offsets.h`) lists
//! `kRvaMouseScreenX = 0x3ED970`, `kRvaMouseScreenY = 0x3ED978`. Build
//! drift may have moved them; this probe just confirms whether those
//! RVAs still hold readable floats.
//!
//! Modes (env-driven):
//! - default: discovery. Reads both, asserts no error from probe.
//! - `HORSEY_EXPECT_FINITE=1`: strict. Assert both values are finite
//!   (not NaN / inf). The default is permissive because the values
//!   may be uninitialized garbage at main menu; once a save is loaded
//!   they should always be finite.
//! - `HORSEY_EXPECT_X_RANGE=<lo..hi>` / `HORSEY_EXPECT_Y_RANGE=<lo..hi>`:
//!   strict. Assert each value is within the named range (decimal
//!   floats, separator `..`). Use to regression-protect against the
//!   HLT-published RVAs drifting onto unrelated memory.

mod common;

use serde_json::json;

#[test]
fn probe_mouse_globals() {
    let Some(game) = common::launch("hk1_probe_mouse_globals") else {
        return;
    };
    eprintln!("[GATE] waiting for a save to load (up to 120s) ...");
    common::wait_for_target_horse(&game, std::time::Duration::from_secs(120));
    let resp = game
        .op_json("hk1.probe.mouse_globals", &json!({}))
        .expect("hk1.probe.mouse_globals op should succeed");
    eprintln!("\nfull response: {resp}");
    let result = resp.get("result").unwrap_or(&resp);
    let x = result.get("x").and_then(|v| v.as_f64());
    let y = result.get("y").and_then(|v| v.as_f64());
    let x_bits = result.get("x_bits").and_then(|v| v.as_str()).unwrap_or("?");
    let y_bits = result.get("y_bits").and_then(|v| v.as_str()).unwrap_or("?");
    eprintln!("mouse_x = {x:?} (bits {x_bits})");
    eprintln!("mouse_y = {y:?} (bits {y_bits})");

    assert!(result.get("error").is_none(), "probe returned an error");

    if std::env::var("HORSEY_EXPECT_FINITE").ok().as_deref() == Some("1") {
        let xv = x.expect("x missing");
        let yv = y.expect("y missing");
        assert!(xv.is_finite(), "HORSEY_EXPECT_FINITE: x={xv} not finite");
        assert!(yv.is_finite(), "HORSEY_EXPECT_FINITE: y={yv} not finite");
    }

    for (name, val) in [("HORSEY_EXPECT_X_RANGE", x), ("HORSEY_EXPECT_Y_RANGE", y)] {
        if let Ok(s) = std::env::var(name) {
            let (lo, hi) = s
                .split_once("..")
                .unwrap_or_else(|| panic!("{name}='{s}' must be lo..hi"));
            let lo: f64 = lo.parse().unwrap_or_else(|e| panic!("{name} lo: {e}"));
            let hi: f64 = hi.parse().unwrap_or_else(|e| panic!("{name} hi: {e}"));
            let v = val.unwrap_or_else(|| panic!("{name} set but value missing from probe"));
            assert!(v >= lo && v <= hi, "{name}={s}: observed {v} outside range");
        }
    }

    game.pass(&format!(
        "mouse_x={:?} mouse_y={:?}",
        x.unwrap_or(f64::NAN),
        y.unwrap_or(f64::NAN)
    ));
}
