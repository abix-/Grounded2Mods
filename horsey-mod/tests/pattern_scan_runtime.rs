//! Runtime pattern-scan test. Drives R1: replace hardcoded RVAs
//! with byte-signature lookups against the loaded `Horsey.exe`
//! image.
//!
//! **Critical: the scanner must run INSIDE the game process.** The
//! first cut of this test ran `modforge::patterns::scan_loaded_image`
//! directly from the test executable, which scanned the test's own
//! .text section (useless). This version drives the mod through
//! HTTP ops `patterns.scan` + `patterns.read_bytes`, both of which
//! run inside Horsey.exe via the injected DLL.
//!
//! Contracts asserted:
//!   1. `patterns.scan` scans the GAME's .text section.
//!   2. Given a 32-byte signature read from a known-good function
//!      entry, scanning round-trips back to that exact address.
//!   3. An absent signature returns null.

mod common;

use modforge::harness::GameHarness;

use modforge::harness::RunningGame;
use serde_json::{Value, json};

/// Ask the mod to read `n` bytes at a runtime address and return
/// them as a hex-byte string. Runs inside the GAME process via the
/// `patterns.read_bytes` op.
fn fetch_signature_at(game: &RunningGame, addr: &str, n: u32) -> String {
    let resp = game
        .op_json("patterns.read_bytes", &json!({"addr": addr, "n": n}))
        .expect("read_bytes op should succeed");
    let bytes = resp
        .get("result")
        .and_then(|r| r.get("bytes"))
        .and_then(|v| v.as_str())
        .expect("read_bytes result.bytes must be a string");
    bytes.to_string()
}

/// Ask the mod to scan its own .text section for `sig`. Returns
/// the matched runtime address (hex string), or `None` if absent.
fn scan(game: &RunningGame, sig: &str) -> Option<String> {
    let resp = game
        .op_json("patterns.scan", &json!({"sig": sig}))
        .expect("scan op should succeed");
    resp.get("result")
        .and_then(|r| r.get("found"))
        .and_then(|v| if v.is_null() { None } else { v.as_str() })
        .map(|s| s.to_string())
}

fn fetch_dryrun_target_addr(game: &RunningGame, op: &str, path: &[&str]) -> String {
    let resp = game
        .op_json(op, &json!({}))
        .expect("dryrun op should succeed");
    let mut cur: &Value = resp.get("result").expect("result");
    for k in path {
        cur = cur
            .get(*k)
            .unwrap_or_else(|| panic!("dryrun {op} missing path segment {k} (full resp: {resp})"));
    }
    cur.as_str()
        .expect("runtime_addr must be a string")
        .to_string()
}

#[test]
fn scan_finds_combinator_via_self_derived_signature() {
    if modforge::harness::should_skip() {
        eprintln!("MODFORGE_NO_GAME=1, skipping");
        return;
    }
    let spec = common::spec();
    let game =
        GameHarness::launch(&spec, "pattern_scan_combinator").expect("harness launch failed");

    // 1. Get combinator's known runtime address from the dryrun.
    let expected =
        fetch_dryrun_target_addr(&game, "genes.ext.combinator.dryrun", &["runtime_addr"]);
    // 2. Read 32 bytes at that address (in the game process).
    let sig = fetch_signature_at(&game, &expected, 32);
    game.log()
        .event("SCAN", &format!("expected={expected} sig=[{sig}]"));

    // 3. Scan the .text section for the signature; assert the
    //    match address equals what we read from.
    let found = scan(&game, &sig);
    game.log().event("SCAN", &format!("found={found:?}"));
    assert_eq!(
        found.as_deref(),
        Some(expected.as_str()),
        "32-byte self-derived signature must round-trip"
    );
    game.pass("self-derived combinator signature round-trips");
}

#[test]
fn scan_finds_apply_gene_to_horse_via_self_derived_signature() {
    if modforge::harness::should_skip() {
        eprintln!("MODFORGE_NO_GAME=1, skipping");
        return;
    }
    let spec = common::spec();
    let game = GameHarness::launch(&spec, "pattern_scan_apply").expect("harness launch failed");

    // D5 render-trampoline dryrun returns the target directly in
    // `result.runtime_addr` (no nested `name` object).
    let expected = fetch_dryrun_target_addr(&game, "genes.ext.render.dryrun", &["runtime_addr"]);
    let sig = fetch_signature_at(&game, &expected, 32);
    game.log()
        .event("SCAN", &format!("expected={expected} sig=[{sig}]"));

    let found = scan(&game, &sig);
    game.log().event("SCAN", &format!("found={found:?}"));
    assert_eq!(
        found.as_deref(),
        Some(expected.as_str()),
        "self-derived APPLY_GENE_TO_HORSE signature must round-trip"
    );
    game.pass("self-derived apply-gene signature round-trips");
}

#[test]
fn scan_returns_none_for_missing_pattern() {
    if modforge::harness::should_skip() {
        eprintln!("MODFORGE_NO_GAME=1, skipping");
        return;
    }
    let spec = common::spec();
    let game = GameHarness::launch(&spec, "pattern_scan_missing").expect("harness launch failed");

    let absent_sig = "ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff";
    let found = scan(&game, absent_sig);
    game.log()
        .event("SCAN", &format!("absent-sig result {found:?}"));
    assert!(
        found.is_none(),
        "absent signature must return None, got {found:?}"
    );
    game.pass("missing signature -> None");
}
