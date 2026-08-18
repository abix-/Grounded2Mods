// Thread-RIP-by-module attribution. Calls the
// `sample_thread_modules` op for 30s @ 10 Hz; pretty-prints the
// per-thread + grand-total breakdown. All decoding + table
// formatting lives in `ueforge::client`.
//
//   set BBP_DEBUG_PORT=17171
//   cargo test --release --test explore_thread_attribution. --nocapture

mod common;

use std::io::Write;

use common::open_perf_log;
use ueforge::client;

#[test]
fn sample_thread_modules() {
    let api = common::Api::require();
    let mut out = open_perf_log("thread_attribution");

    writeln!(out, "=== Sampling thread modules for 30s @ 10 Hz ===").unwrap();
    writeln!(
        out,
        "(suspending each thread briefly to read RIP, then resuming)"
    )
    .unwrap();

    let report = client::sample_thread_modules(api.inner(), 30_000, 100);
    writeln!(out, "{report}").unwrap();
}
