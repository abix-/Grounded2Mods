//! Regression test for "year and sleeps flicker to byte-pattern
//! garbage" seen 2026-05-15.
//!
//! Symptom: pre-fix, the HTTP snapshot reported `world_loaded: true`
//! whenever the rebased GameState struct existed in `.data` (always),
//! and the year/sleeps reads returned whatever bytes happened to be
//! at offsets 0x314/0x318. During main-menu / transient states the
//! game writes a fill pattern there, producing values like
//! year=16842752 (0x01010100) and sleeps=65793 (0x00010101).
//!
//! Two-pass contract:
//!
//!   Pass 1: at main menu (no save loaded), `world_loaded` MUST be
//!   `false` and `money/year/sleeps` MUST all be `null`. Without
//!   this, the UI bleeds byte-pattern leaks into the display.
//!
//!   Pass 2 (when actually loaded, future): consecutive snapshots
//!   taken < 100ms apart must report stable year + sleeps (game
//!   advances at a sleep cycle, not per-frame). Sanity bounds:
//!   year < 10_000, sleeps < 10_000_000. Year=16842752 fails this
//!   trivially.
//!
//! The harness drops us at the main menu, so Pass 1 runs in CI. Pass 2
//! is gated on `world_loaded` so it skips harmlessly when no save is
//! loaded.

mod common;

use serde_json::{Value, json};

fn opt_u64(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(|v| v.as_u64())
}

fn capture(game: &modforge::harness::RunningGame) -> Value {
    let resp = game.op_json("ping", &json!({})).expect("ping must succeed");
    resp.get("state")
        .cloned()
        .expect("ping response must include state")
}

#[test]
fn snapshot_does_not_leak_unloaded_garbage() {
    let Some(game) = common::launch("snapshot_stability") else {
        return;
    };

    let s = capture(&game);
    let loaded = s
        .get("world_loaded")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    if !loaded {
        // Main-menu state: every read-from-GameState field must be
        // null, otherwise we're back to bleeding garbage into the UI.
        for field in ["money", "year", "sleeps"] {
            assert!(
                s.get(field).map(|v| v.is_null()).unwrap_or(false),
                "world_loaded=false but {field} is not null: {s}"
            );
        }
        game.pass("main-menu snapshot is clean (no garbage leak)");
        return;
    }

    // Save IS loaded (rare in CI but covered). Stability + sanity.
    let y0 = opt_u64(&s, "year");
    let sl0 = opt_u64(&s, "sleeps");

    std::thread::sleep(std::time::Duration::from_millis(50));
    let s2 = capture(&game);
    let y1 = opt_u64(&s2, "year");
    let sl1 = opt_u64(&s2, "sleeps");

    assert_eq!(y0, y1, "year flickered: {s} -> {s2}");
    assert_eq!(sl0, sl1, "sleeps flickered: {s} -> {s2}");

    if let Some(y) = y0 {
        assert!(y < 10_000, "year out of sanity bounds: {y} in {s}");
    }
    if let Some(sl) = sl0 {
        assert!(sl < 10_000_000, "sleeps out of sanity bounds: {sl} in {s}");
    }
    game.pass("loaded snapshot is stable + within sanity bounds");
}
