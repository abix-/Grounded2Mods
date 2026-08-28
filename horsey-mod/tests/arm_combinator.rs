//! Thin wrapper over [`modforge::testkit::arm_lifecycle::run`] for the
//! D3.4 breeding-combinator detour.
//!
//! The combinator fires whenever vanilla creates a child horse: birth
//! events, CRISPR clone, pop respawn. Idling at the menu typically
//! sees zero fires; the test just proves arm+disarm survives.

mod common;

use modforge::testkit::arm_lifecycle::{self, Config, CounterBound, SubsystemOps};
use std::time::Duration;

#[test]
fn arm_combinator_does_not_crash() {
    let Some(game) = common::launch("arm_combinator") else {
        return;
    };
    let mut cfg = Config::defaults(
        SubsystemOps::from_prefix("genes.ext.combinator"),
        "combinator",
    );
    cfg.idle = Duration::from_secs(3);
    cfg.counter_bounds = vec![
        CounterBound {
            key: "call_count".into(),
            max: 1_000_000,
        },
        CounterBound {
            key: "combines_done".into(),
            max: 1_000_000,
        },
    ];
    let report = arm_lifecycle::run(&game, &cfg).unwrap_or_else(|e| panic!("{e}"));

    // Additional combinator-specific invariant: every fire must reach
    // the slow path (combines_done == call_count).
    let call_count = report
        .stats
        .get("call_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let combines = report
        .stats
        .get("combines_done")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(
        call_count, combines,
        "combines_done ({combines}) != call_count ({call_count})"
    );

    game.pass("combinator arm + idle + disarm survived without crash");
}
