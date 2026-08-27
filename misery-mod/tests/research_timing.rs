//! What is the mod actually costing the running game?
//!
//! `docs/performance.md` lists everything the mod does with an
//! ESTIMATE against each one, read off the code. This replaces
//! the estimates with numbers from your session.
//!
//! Timing is off by default. These tests switch it on, watch for
//! a window, read the report, and switch it off again, so nothing
//! is left running afterwards.
//!
//! Two numbers matter:
//!
//! - **Time on the game thread per second of play.** A frame at
//!   60 fps is 16 ms. If the mod holds the game thread for
//!   hundreds of milliseconds a second, the mod is the stutter.
//! - **The worst single run.** An average hides a stall that
//!   happens once a minute, and that is exactly what a player
//!   notices.
//!
//! Run WHILE PLAYING, with a world loaded:
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_timing -- --test-threads=1 --nocapture
//! ```

mod common;
use common::api_or_skip;
use serde_json::json;
use std::time::{Duration, Instant};

/// Long enough to catch the watchers, which run every 5 seconds.
const WINDOW: Duration = Duration::from_secs(30);

/// Time everything for a window and print what it cost.
#[test]
fn what_the_mod_costs() {
    let Some(api) = api_or_skip() else { return };

    let on = api.op("timing", json!({ "on": true, "reset": true }));
    assert!(on.ok, "could not switch timing on: {:?}", on.error);

    // The game thread total is counted whether timing is on or
    // not, so take it either side of the window.
    let before = drain_ms(&api);
    let start = Instant::now();
    println!("timing for {WINDOW:?}. Play, do not sit in a menu.");
    std::thread::sleep(WINDOW);
    let elapsed = start.elapsed();
    let after = drain_ms(&api);

    let report = api.op("timing_report", json!({}));
    assert!(report.ok, "timing_report failed: {:?}", report.error);
    let off = api.op("timing", json!({ "on": false, "reset": false }));
    assert!(off.ok, "could not switch timing off: {:?}", off.error);

    let held = after - before;
    let secs = elapsed.as_secs_f64();
    println!("\n=== game thread ===");
    println!("held for {held:.1} ms over {secs:.1} s");
    println!("that is {:.2} ms per second of play", held / secs);
    println!("a frame at 60 fps is 16.7 ms");

    println!("\n=== by name, slowest first ===");
    println!(
        "{:<34} {:>8} {:>11} {:>10} {:>11}",
        "name", "calls", "total ms", "avg us", "worst ms"
    );
    for e in report.result["entries"].as_array().cloned().unwrap_or_default() {
        println!(
            "{:<34} {:>8} {:>11.2} {:>10.1} {:>11.2}",
            e["name"].as_str().unwrap_or("?"),
            e["calls"],
            e["total_ms"].as_f64().unwrap_or(0.0),
            e["avg_us"].as_f64().unwrap_or(0.0),
            e["worst_ms"].as_f64().unwrap_or(0.0),
        );
    }

    // Not an assertion about being fast: an assertion that the
    // measurement happened at all. A silent empty report would
    // otherwise read as "nothing costs anything".
    assert!(
        report.result["measured"].as_u64().unwrap_or(0) > 0,
        "nothing was measured; is a world loaded and are the watchers running?"
    );
}

/// Total milliseconds the mod has held the game thread since
/// launch.
fn drain_ms(api: &common::Api) -> f64 {
    let r = api.op("pe_stats", json!({}));
    assert!(r.ok, "pe_stats failed: {:?}", r.error);
    r.result["queued_work_ms"].as_f64().unwrap_or(0.0)
}

/// Timing must cost nothing when it is off.
///
/// The point of the switch is that the calls can be left in the
/// code permanently. If an off report still filled up, they could
/// not be.
#[test]
fn off_means_off() {
    let Some(api) = api_or_skip() else { return };
    let off = api.op("timing", json!({ "on": false, "reset": true }));
    assert!(off.ok, "could not switch timing off: {:?}", off.error);
    std::thread::sleep(Duration::from_secs(6));
    let r = api.op("timing_report", json!({}));
    assert!(r.ok, "timing_report failed: {:?}", r.error);
    println!("{}", serde_json::to_string_pretty(&r.result).unwrap_or_default());
    assert_eq!(r.result["timing_on"], json!(false));
    let busy: Vec<&serde_json::Value> = r.result["entries"]
        .as_array()
        .map(|a| a.iter().filter(|e| e["calls"].as_u64().unwrap_or(0) > 0).collect())
        .unwrap_or_default();
    assert!(busy.is_empty(), "timing is off but something recorded: {busy:?}");
}
