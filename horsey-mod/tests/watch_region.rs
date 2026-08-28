//! Thin wrapper over [`modforge::testkit::watch::run_region`]. See
//! that module for the full env-var contract and three-mode semantics.
//!
//! Horsey-specific bits: launching the harness and resolving the
//! `HORSEY_WATCH_BASE` spec via [`common::resolve_base`].

mod common;

use modforge::testkit::watch;
use std::time::Duration;

#[test]
fn watch_region() {
    let Some(game) = common::launch("watch_region") else {
        return;
    };
    common::wait_for_target_horse(&game, Duration::from_secs(120));

    let env = watch::EnvRegionConfig::from_env("HORSEY_WATCH").expect("parse HORSEY_WATCH_* env");
    let base = common::resolve_base(&game, &env.base_spec);
    let expect = env.expect;
    let timeout_ms = env.timeout_ms;
    let cfg = env.resolve(base);

    let result = watch::run_region(&game, &cfg).expect("watch::run_region");
    result
        .assert_or_err(expect, timeout_ms)
        .unwrap_or_else(|e| panic!("{e}"));
}
