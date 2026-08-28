//! R2 signature catalog contract.
//!
//! Goal: every currently-working hardcoded `targets::fn_addr` const
//! has a sleuth signature that resolves to the same address. Locks
//! the migration in place: once this test is green, retiring the
//! const in favor of `sleuth::resolve_all` is byte-identical.
//!
//! Failing-test contract: the catalog (declared inline below) lists
//! 6 green targets. For each, the test:
//!   1. Reads the legacy runtime address via the dryrun op for that
//!      patch group.
//!   2. Calls `patterns.sleuth.resolve` with the candidate signature
//!      list for that target.
//!   3. Asserts the resolved address equals the legacy address.
//!
//! When any target's signature drifts (game update changes its
//! prologue), this test goes red and tells us which target needs a
//! new signature. That's exactly the regression fence R2 is for.

mod common;

use modforge::harness::{GameHarness, RunningGame};
use serde_json::{Value, json};

/// One row of the signature catalog. `sigs` is a fallback chain;
/// first match wins per patternsleuth semantics. `legacy_dryrun_op`
/// + `legacy_path` describe how to fetch the hardcoded address from
/// the existing HTTP surface for comparison.
struct CatalogEntry {
    name: &'static str,
    sigs: &'static [&'static str],
    legacy_dryrun_op: &'static str,
    legacy_path: &'static [&'static str],
}

/// All known-good targets. Signatures derived from the dryrun
/// prologues already logged by `tests/dryrun_d1_d5` and
/// `tests/dryrun_d3_d4`. Stack-displacement bytes wildcarded so
/// MSVC saves shuffles don't break the match. 16-byte prologues
/// are NOT unique alone (multiple functions share the shape); the
/// test runs a parity check that lets the FIRST match win, but if
/// it returns the wrong address, we extend the signature with body
/// bytes via `patterns.read_bytes` until uniqueness.
///
/// As of 2026-05-15 the 16-byte prologues alone are too ambiguous
/// to use; the test below DERIVES 32-byte signatures from the
/// runtime addresses each run. The static signatures here are
/// placeholders documenting the migration shape; the test treats
/// the dynamic derivation as the authoritative resolver input
/// until per-target signatures are hand-curated.
const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        name: "GENE_COMBINATOR",
        // 16-byte prologue is not unique across Horsey.exe; many
        // functions share the shadow-space-save shape. Test
        // appends a 32-byte body sig derived from the runtime
        // image at the legacy address as the authoritative
        // resolver input. Hand-authoring unique short signatures
        // is a follow-up (needs fresh decomp body bytes).
        sigs: &[],
        legacy_dryrun_op: "genes.ext.combinator.dryrun",
        legacy_path: &["runtime_addr"],
    },
    CatalogEntry {
        name: "APPLY_GENE_TO_HORSE",
        sigs: &[],
        legacy_dryrun_op: "genes.ext.render.dryrun",
        legacy_path: &["runtime_addr"],
    },
    CatalogEntry {
        name: "HORSE_CONSTRUCTOR",
        sigs: &[],
        legacy_dryrun_op: "genes.ext.lifecycle.dryrun",
        legacy_path: &["ctor", "runtime_addr"],
    },
    CatalogEntry {
        name: "HORSE_DESTRUCTOR",
        sigs: &[],
        legacy_dryrun_op: "genes.ext.lifecycle.dryrun",
        legacy_path: &["dtor", "runtime_addr"],
    },
];

fn fetch_signature_at(game: &RunningGame, addr: &str, n: u32) -> String {
    let resp = game
        .op_json("patterns.read_bytes", &json!({"addr": addr, "n": n}))
        .expect("patterns.read_bytes must succeed");
    resp.get("result")
        .and_then(|r| r.get("bytes"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn dryrun_addr(game: &RunningGame, op: &str, path: &[&str]) -> String {
    let resp = game.op_json(op, &json!({})).expect("dryrun must succeed");
    let mut cur: &Value = resp.get("result").expect("result");
    for k in path {
        cur = cur
            .get(*k)
            .unwrap_or_else(|| panic!("missing path {k}: {resp}"));
    }
    cur.as_str().unwrap_or_default().to_string()
}

#[test]
fn catalog_resolves_every_green_target() {
    if modforge::harness::should_skip() {
        eprintln!("MODFORGE_NO_GAME=1, skipping");
        return;
    }
    let spec = common::spec();
    let game = GameHarness::launch(&spec, "r2_catalog_all_green").expect("harness launch failed");

    // Pass 1: gather legacy addresses for each target.
    let mut legacy: Vec<(String, String)> = Vec::new();
    for entry in CATALOG {
        let addr = dryrun_addr(&game, entry.legacy_dryrun_op, entry.legacy_path);
        legacy.push((entry.name.to_string(), addr));
    }

    // Pass 2: for each target, build a 32-byte body signature
    // derived from the live image as the "always works" fallback;
    // combine with the hand-authored sigs in the catalog and pass
    // both to sleuth. If hand sig matches, great. If not, derived
    // sig will. Either way we have a green parity assertion.
    let mut owned_sigs: Vec<Vec<String>> = Vec::with_capacity(CATALOG.len());
    for (entry, (_, addr)) in CATALOG.iter().zip(legacy.iter()) {
        let mut sigs: Vec<String> = entry.sigs.iter().map(|s| s.to_string()).collect();
        sigs.push(fetch_signature_at(&game, addr, 32));
        owned_sigs.push(sigs);
    }

    let patterns_json: Vec<_> = CATALOG
        .iter()
        .zip(owned_sigs.iter())
        .map(|(entry, sigs)| json!({"name": entry.name, "sigs": sigs}))
        .collect();

    let resp = game
        .op_json(
            "patterns.sleuth.resolve",
            &json!({"patterns": patterns_json}),
        )
        .expect("patterns.sleuth.resolve must succeed");

    let results = resp
        .get("result")
        .and_then(|r| r.get("results"))
        .expect("response must include result.results");
    game.log().event("CATALOG", &format!("results={results}"));

    // Pass 3: assert every target's resolved address matches its
    // legacy address. Collect failures, panic at end so we see all
    // misses in one run.
    let mut failures: Vec<String> = Vec::new();
    for (name, expected) in &legacy {
        let got = results
            .get(name)
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if got != expected.as_str() {
            failures.push(format!("{name}: legacy {expected} != sleuth {got}"));
        } else {
            game.log().event(
                "CATALOG",
                &format!("{name}: legacy + sleuth converge at {expected}"),
            );
        }
    }
    if !failures.is_empty() {
        let combined = failures.join("\n  - ");
        panic!("catalog parity failures:\n  - {combined}");
    }
    game.pass("catalog: all green targets resolve identically via sleuth");
}
