//! Thin wrapper over [`modforge::testkit::watch::run_single`]. See
//! that module for the full env-var contract.

mod common;

use modforge::testkit::watch;
use std::time::Duration;

#[test]
fn watch_until_change() {
    let Some(game) = common::launch("watch_until_change") else {
        return;
    };
    common::wait_for_target_horse(&game, Duration::from_secs(120));

    let env = watch::EnvSingleConfig::from_env("HORSEY_WATCH").expect("parse HORSEY_WATCH_* env");
    let base = common::resolve_base(&game, &env.base_spec);
    let expect = env.expect;
    let cfg = env.resolve(base);
    let budget_ms = cfg.budget_ms;

    let result = watch::run_single(&game, &cfg).expect("watch::run_single");
    result
        .assert_or_err(expect, budget_ms)
        .unwrap_or_else(|e| panic!("{e}"));
}
