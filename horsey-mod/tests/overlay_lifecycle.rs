//! Hudhook overlay live test. Attaches to a running, mod-injected
//! Horsey.exe, polls `ui.overlay.stats` over a short window, and
//! asserts the overlay is armed and rendering frames into the game's
//! swap chain.
//!
//! Run with the mod already injected:
//!   cargo run -p horsey-mod --bin horsey-play --release
//!   set MODFORGE_ATTACH=1
//!   cargo test -p horsey-mod --test overlay_lifecycle. --nocapture
//!
//! This is the live verification gate for the hudhook in-game overlay.
//! See `docs/IN-GAME-OVERLAY-PLAN.md`.

mod common;

use modforge::harness::GameHarness;
use serde_json::{Value, json};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn overlay_armed_and_advancing() {
    if modforge::harness::should_skip() {
        eprintln!("MODFORGE_NO_GAME=1, skipping live-overlay test");
        return;
    }
    let spec = common::spec();
    let game = GameHarness::attach_existing(&spec, "overlay_armed_and_advancing")
        .expect("expected a running horsey-mod on 127.0.0.1:33077");

    // Snapshot the initial frame count, then wait for the render
    // thread to advance it. We poll for up to 5 seconds and accept
    // anything past the initial value by at least 30 frames (roughly
    // 0.5 seconds of frames at typical refresh).
    let first: Value = game
        .op_json("ui.overlay.stats", &json!({}))
        .expect("ui.overlay.stats op should succeed");
    game.pass(&format!("initial envelope: {first}"));

    let result = &first["result"];
    assert_eq!(
        result.get("armed").and_then(Value::as_bool),
        Some(true),
        "overlay should be armed (was lib.rs::worker_main arming it?)"
    );

    let start_frames = result
        .get("frames")
        .and_then(Value::as_u64)
        .expect("result.frames should be a u64");

    let deadline = Instant::now() + Duration::from_secs(5);
    let target = start_frames + 30;
    let mut latest = start_frames;
    while Instant::now() < deadline {
        let stats: Value = game
            .op_json("ui.overlay.stats", &json!({}))
            .expect("ui.overlay.stats op should succeed");
        latest = stats["result"]
            .get("frames")
            .and_then(Value::as_u64)
            .unwrap_or(start_frames);
        if latest >= target {
            game.pass(&format!(
                "overlay frame count advanced {} -> {} (delta {})",
                start_frames,
                latest,
                latest - start_frames
            ));
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }

    panic!(
        "overlay frame count did not advance: start={start_frames} latest={latest} \
         (expected delta >= 30 within 5s). Render thread is not calling Present \
         through the hudhook detour, or hudhook never installed the hooks."
    );
}
