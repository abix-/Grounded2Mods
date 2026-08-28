//! Thin wrapper over [`modforge::testkit::arm_lifecycle::run`] for
//! the D3.1/D3.2 lifecycle anchors (horse ctor + dtor). Demonstrates
//! the multi-target / multi-flag variant of the shared flow.

mod common;

use modforge::testkit::arm_lifecycle::{self, Config, CounterBound, SubsystemOps};
use std::time::Duration;

#[test]
fn arm_lifecycle_fires_on_horse_creation() {
    let Some(game) = common::launch("arm_lifecycle") else {
        return;
    };
    let mut cfg = Config::defaults(
        SubsystemOps::from_prefix("genes.ext.lifecycle"),
        "lifecycle",
    );
    cfg.idle = Duration::from_secs(5);
    // Lifecycle dryrun has TWO targets (ctor + dtor), each with its
    // own prologue under `result.{ctor|dtor}.prologue_bytes`.
    cfg.prologue_paths = vec!["ctor.prologue_bytes".into(), "dtor.prologue_bytes".into()];
    // Arm/disarm responses carry separate flags per target.
    cfg.armed_paths = vec!["armed_ctor".into(), "armed_dtor".into()];
    cfg.counter_bounds = vec![
        CounterBound {
            key: "ctor_calls".into(),
            max: 1_000_000,
        },
        CounterBound {
            key: "dtor_calls".into(),
            max: 1_000_000,
        },
    ];

    let report = arm_lifecycle::run(&game, &cfg).unwrap_or_else(|e| panic!("{e}"));

    // Lifecycle-specific invariant: every ctor invocation creates
    // exactly one EXT_HORSE_GENOMES entry, every dtor drops one.
    let ctor = report
        .stats
        .get("ctor_calls")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let dtor = report
        .stats
        .get("dtor_calls")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let created = report
        .stats
        .get("entries_created")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let dropped = report
        .stats
        .get("entries_dropped")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(
        ctor, created,
        "entries_created ({created}) != ctor_calls ({ctor})"
    );
    assert_eq!(
        dtor, dropped,
        "entries_dropped ({dropped}) != dtor_calls ({dtor})"
    );

    game.pass("lifecycle arm + idle + disarm survived; counters consistent");
}
