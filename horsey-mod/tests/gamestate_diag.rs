//! Forensic test for `gamestate::looks_loaded` against live game state.
//!
//! Replaces ad-hoc curl probes with an in-tree test that:
//!
//!   1. Runs `gamestate.diag` against the running game.
//!   2. Logs the full dump to the per-run harness log so failure
//!      diagnosis is the test artifact, not a Discord screenshot.
//!   3. Asserts internal consistency that MUST hold regardless of
//!      whether the user is at the main menu or in a save:
//!
//!        - `verdict_looks_loaded` equals `roster.looks_loaded` (they
//!          are the same function; this catches accidental drift).
//!        - `fields.money/year/sleeps` are u32-valued (Some). The
//!          struct is mapped, so reads can't return None. The
//!          "garbage values when not loaded" problem is a SEMANTICS
//!          issue, not a read failure.
//!        - When `verdict_looks_loaded` is true, `roster.count_if_sane`
//!          is `Some` and < 2560 (the bound the heuristic enforces).
//!        - When `verdict_looks_loaded` is false, EITHER the roster
//!          span is bogus OR the count exceeds the bound. We assert
//!          one of those is true so a future bug that makes the
//!          heuristic flip without explanation gets caught.
//!
//!   4. Cross-check against `MODFORGE_EXPECT_LOADED`: if the env var
//!      is set, the user has told us "I am in a save right now."
//!      The test then DEMANDS `verdict_looks_loaded == true` and
//!      fails loudly with the diag dump if it isn't. This is the
//!      regression test for the 2026-05-15 false-negative the user
//!      hit ("I have a save running but the UI says no save loaded").
//!
//!      Without `MODFORGE_EXPECT_LOADED`, the test only checks
//!      internal consistency, so CI runs (which sit at the main
//!      menu) still pass cleanly.

mod common;

use serde_json::{Value, json};

fn diag(game: &modforge::harness::RunningGame) -> Value {
    let resp = game
        .op_json("gamestate.diag", &json!({}))
        .expect("gamestate.diag must succeed");
    resp.get("result")
        .cloned()
        .expect("diag response missing result")
}

fn b(v: &Value, key: &str) -> bool {
    v.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

#[test]
fn looks_loaded_is_internally_consistent_with_diag() {
    let Some(game) = common::launch("gamestate_diag") else {
        return;
    };
    let d = diag(&game);
    game.log().event("DIAG", &format!("gamestate.diag = {d:#}"));

    let verdict = b(&d, "verdict_looks_loaded");
    let roster_loaded = d
        .get("roster")
        .map(|r| b(r, "looks_loaded"))
        .unwrap_or(false);

    assert_eq!(
        verdict, roster_loaded,
        "verdict_looks_loaded ({verdict}) != roster.looks_loaded ({roster_loaded}); \
         the heuristic split. diag = {d:#}"
    );

    let fields = d.get("fields").expect("fields object");
    for f in ["money", "year", "sleeps"] {
        assert!(
            fields.get(f).and_then(|v| v.as_u64()).is_some(),
            "fields.{f} must be Some (struct is always mapped); diag = {d:#}"
        );
    }

    let roster = d.get("roster").expect("roster object");
    if verdict {
        // True case: the span must have produced a sane count under
        // the bound the heuristic enforces.
        let count = roster
            .get("count_if_sane")
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| panic!("verdict=true but count_if_sane is null; diag = {d:#}"));
        assert!(
            count < 2560,
            "verdict=true but count {count} >= 2560 bound; diag = {d:#}"
        );
    } else {
        // False case: at least one of the heuristic's rejection
        // reasons must apply. Catches bug-by-omission.
        let begin = roster
            .get("begin")
            .and_then(|v| v.as_str())
            .and_then(|s| usize::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .unwrap_or(0);
        let end = roster
            .get("end")
            .and_then(|v| v.as_str())
            .and_then(|s| usize::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .unwrap_or(0);
        let too_small = begin < 0x10000;
        let inverted = end < begin;
        let misaligned = end >= begin && (end - begin) % 0x24 != 0;
        let over_cap = end >= begin && (end - begin) % 0x24 == 0 && (end - begin) / 0x24 >= 2560;
        assert!(
            too_small || inverted || misaligned || over_cap,
            "verdict=false but no rejection reason fired; diag = {d:#}"
        );
    }
    game.pass("gamestate.diag self-consistent with looks_loaded");
}

#[test]
fn user_says_loaded_then_heuristic_agrees() {
    // Gated on env so CI (main menu) doesn't false-fail. Set
    // `MODFORGE_EXPECT_LOADED=1` when launching the test while the
    // game is parked in a real save.
    if std::env::var("MODFORGE_EXPECT_LOADED").ok().as_deref() != Some("1") {
        eprintln!("MODFORGE_EXPECT_LOADED!=1, skipping in-save assertion");
        return;
    }
    let Some(game) = common::launch("gamestate_diag_loaded") else {
        return;
    };
    let d = diag(&game);
    game.log()
        .event("DIAG-LOADED", &format!("gamestate.diag = {d:#}"));
    let verdict = b(&d, "verdict_looks_loaded");
    assert!(
        verdict,
        "user reported a save was loaded but looks_loaded() = false; \
         the heuristic is wrong for this build state. diag = {d:#}"
    );
}
