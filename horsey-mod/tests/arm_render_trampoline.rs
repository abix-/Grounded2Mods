//! Thin wrapper over [`modforge::testkit::arm_lifecycle::run`] for the
//! D5 render-trampoline post-hook on `APPLY_GENE_TO_HORSE`.
//!
//! The render trampoline is HOT: vanilla calls it once per visible
//! horse per frame. Idling 3 seconds at the main menu with demo
//! horses should accumulate hundreds of calls.

mod common;

use modforge::testkit::arm_lifecycle::{self, Config, CounterBound, SubsystemOps};
use std::time::Duration;

#[test]
fn arm_render_trampoline_fires_and_disarms_cleanly() {
    let Some(game) = common::launch("arm_render_trampoline") else {
        return;
    };
    let mut cfg = Config::defaults(
        SubsystemOps::from_prefix("genes.ext.render"),
        "render trampoline",
    );
    cfg.idle = Duration::from_secs(3);
    cfg.counter_bounds = vec![CounterBound {
        key: "call_count".into(),
        max: 10_000_000,
    }];
    let report = arm_lifecycle::run(&game, &cfg).unwrap_or_else(|e| panic!("{e}"));

    // Render-specific invariant: with zero ext genes authored,
    // genes_applied_total stays at 0 and must never exceed call_count.
    let call_count = report
        .stats
        .get("call_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let applied = report
        .stats
        .get("genes_applied_total")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        applied <= call_count,
        "genes_applied ({applied}) > call_count ({call_count}); accounting bug"
    );

    game.pass("render trampoline arm + idle + disarm survived");
}
