//! R2 contract: `modforge::patterns::sleuth` resolves multiple
//! targets in one shot via the patternsleuth crate.
//!
//! Failing test first. Compile failure / op-not-found failure here
//! is the contract that drives the implementation.
//!
//! What we want:
//!   - HTTP op `patterns.sleuth.resolve` taking
//!     `{patterns: [{name, sigs:[...]}, ...]}` and returning
//!     `{results: {name: "0x..." | null, ...}}`.
//!   - Each target supports multiple signature candidates; the
//!     first that matches wins (lets us survive compiler-reordered
//!     prologues by listing alternate sigs).
//!   - Wraps `patternsleuth::Image::scan` so we get SIMD-accelerated
//!     multi-target scanning.
//!
//! Test strategy: pick two known-good functions (`GENE_COMBINATOR`,
//! `APPLY_GENE_TO_HORSE`). Read 32 bytes at each (via the existing
//! `patterns.read_bytes` op) to build signatures. Resolve both in
//! ONE call. Assert results match the source addresses.

mod common;

use modforge::harness::{GameHarness, RunningGame};
use serde_json::{Value, json};

fn fetch_signature_at(game: &RunningGame, addr: &str, n: u32) -> String {
    let resp = game
        .op_json("patterns.read_bytes", &json!({"addr": addr, "n": n}))
        .expect("patterns.read_bytes op must be available");
    resp.get("result")
        .and_then(|r| r.get("bytes"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn dryrun_addr(game: &RunningGame, op: &str, path: &[&str]) -> String {
    let resp = game
        .op_json(op, &json!({}))
        .expect("dryrun op must succeed");
    let mut cur: &Value = resp.get("result").expect("result");
    for k in path {
        cur = cur
            .get(*k)
            .unwrap_or_else(|| panic!("missing path {k} (resp: {resp})"));
    }
    cur.as_str().unwrap_or_default().to_string()
}

#[test]
fn sleuth_resolves_known_targets_in_one_pass() {
    if modforge::harness::should_skip() {
        eprintln!("MODFORGE_NO_GAME=1, skipping");
        return;
    }
    let spec = common::spec();
    let game = GameHarness::launch(&spec, "r2_resolver").expect("harness launch failed");

    // 1. Get reference addresses + derive signatures.
    let combinator_addr = dryrun_addr(&game, "genes.ext.combinator.dryrun", &["runtime_addr"]);
    let apply_addr = dryrun_addr(&game, "genes.ext.render.dryrun", &["runtime_addr"]);
    let combinator_sig = fetch_signature_at(&game, &combinator_addr, 32);
    let apply_sig = fetch_signature_at(&game, &apply_addr, 32);

    game.log().event(
        "R2",
        &format!("combinator expected={combinator_addr} sig=[{combinator_sig}]"),
    );
    game.log().event(
        "R2",
        &format!("apply expected={apply_addr} sig=[{apply_sig}]"),
    );

    // 2. Resolve BOTH in a single op call. CONTRACT: this op does
    //    not exist yet; the test fails until R2 implements it.
    let resp = game
        .op_json(
            "patterns.sleuth.resolve",
            &json!({
                "patterns": [
                    {"name": "GENE_COMBINATOR", "sigs": [combinator_sig]},
                    {"name": "APPLY_GENE_TO_HORSE", "sigs": [apply_sig]},
                ]
            }),
        )
        .expect("patterns.sleuth.resolve must succeed");

    let results = resp
        .get("result")
        .and_then(|r| r.get("results"))
        .expect("response must have result.results object");
    game.log().event("R2", &format!("results={results}"));

    let got_combinator = results
        .get("GENE_COMBINATOR")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let got_apply = results
        .get("APPLY_GENE_TO_HORSE")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    assert_eq!(
        got_combinator, combinator_addr,
        "sleuth must find combinator at the same address as the source"
    );
    assert_eq!(
        got_apply, apply_addr,
        "sleuth must find apply_gene_to_horse at the same address as the source"
    );
    game.pass("sleuth resolved both targets in one pass");
}

#[test]
fn sleuth_returns_null_for_missing_signature() {
    if modforge::harness::should_skip() {
        eprintln!("MODFORGE_NO_GAME=1, skipping");
        return;
    }
    let spec = common::spec();
    let game = GameHarness::launch(&spec, "r2_resolver_missing").expect("harness launch failed");

    let resp = game
        .op_json(
            "patterns.sleuth.resolve",
            &json!({
                "patterns": [
                    {
                        "name": "DEFINITELY_MISSING",
                        "sigs": ["ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff"],
                    }
                ]
            }),
        )
        .expect("patterns.sleuth.resolve must succeed");

    let results = resp
        .get("result")
        .and_then(|r| r.get("results"))
        .expect("response must have result.results object");
    let got = results.get("DEFINITELY_MISSING");
    assert!(
        got.map(|v| v.is_null()).unwrap_or(false),
        "missing signature must produce null: got {got:?}"
    );
    game.pass("missing signature -> null");
}

#[test]
fn sleuth_first_matching_sig_wins() {
    // Multi-candidate API: if sig A doesn't match but sig B does,
    // the resolver must return B's address. Verifies "list of
    // candidates" behavior.
    if modforge::harness::should_skip() {
        eprintln!("MODFORGE_NO_GAME=1, skipping");
        return;
    }
    let spec = common::spec();
    let game = GameHarness::launch(&spec, "r2_resolver_fallback").expect("harness launch failed");

    let combinator_addr = dryrun_addr(&game, "genes.ext.combinator.dryrun", &["runtime_addr"]);
    let real_sig = fetch_signature_at(&game, &combinator_addr, 32);
    let fake_sig = "ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff";

    let resp = game
        .op_json(
            "patterns.sleuth.resolve",
            &json!({
                "patterns": [
                    {"name": "TARGET", "sigs": [fake_sig, real_sig]},
                ]
            }),
        )
        .expect("patterns.sleuth.resolve must succeed");

    let got = resp
        .get("result")
        .and_then(|r| r.get("results"))
        .and_then(|r| r.get("TARGET"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    assert_eq!(
        got, combinator_addr,
        "second sig matched but resolver did not return its address"
    );
    game.pass("first-match-wins fallback works");
}
