// Performance instrumentation: snapshot at T0, sleep for a
// window while gameplay continues, snapshot at T1. Reports
// counter / cpu / memory / threads / population deltas.
//
// All diff math + table formatting lives in
// `ueforge::client::diff`. This test owns only the window length
// + the perf-log destination.
//
// Run (with the game in-world):
//   set BBP_DEBUG_PORT=17171
//   cargo test --target x86_64-pc-windows-msvc \
//     --test explore_perf_counters. --nocapture --test-threads=1

mod common;

use std::io::Write;
use std::time::Duration;

use common::open_perf_log;
use ueforge::client::diff::MetricsSnapshot;

const WINDOW_SECS: u64 = 30;

#[test]
fn measure_counter_deltas() {
    let api = common::Api::require();
    let mut out = open_perf_log("perf_counters");

    writeln!(out, "\n=== Snapshot T0 ===").unwrap();
    let s0 = MetricsSnapshot::from_api(api.inner());

    writeln!(out, "Sleeping {WINDOW_SECS}s while you play in-game...").unwrap();
    std::thread::sleep(Duration::from_secs(WINDOW_SECS));

    writeln!(out, "\n=== Snapshot T1 ===").unwrap();
    let s1 = MetricsSnapshot::from_api(api.inner());

    let diff = s0.diff_all(&s1);
    writeln!(out, "{}", diff).unwrap();

    writeln!(out, "\n=== Top 5 by event rate ===").unwrap();
    for r in diff.counters.rates.iter().take(5) {
        writeln!(out, "  {:<40} {:.1}/sec", r.name, r.per_sec).unwrap();
    }
    writeln!(out, "\n=== Top 3 by hot-path CPU time ===").unwrap();
    for t in diff.counters.times.iter().take(3) {
        writeln!(
            out,
            "  {:<40} {} ns ({:.3}% wall)",
            t.name, t.ns, t.pct_wall
        )
        .unwrap();
    }
    if diff.cpu.total_ns > 0 {
        let our_share = (diff.counters.our_total_ns as f64) / (diff.cpu.total_ns as f64) * 100.0;
        writeln!(out, "\n  OUR FRACTION OF PROCESS CPU: {:.3}%", our_share).unwrap();
    }
}
