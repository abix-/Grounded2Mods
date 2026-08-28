//! R2 migration contract: arming the combinator detour via a
//! patternsleuth-resolved address must work identically to arming
//! via the hardcoded RVA.
//!
//! This drives the migration pattern: per-game `targets::fn_addr`
//! constants get an alternate signature-driven path; tests force
//! each migration to be byte-identical against the legacy path
//! before retiring the hardcoded address.
//!
//! Failing test contract:
//!   1. Get GENE_COMBINATOR's hardcoded resolution via the existing
//!      `genes.ext.combinator.dryrun` op.
//!   2. Resolve the SAME function via patternsleuth using a body
//!      signature.
//!   3. Assert both addresses match.
//!
//! Once green, we know the resolver gives the same answer as the
//! baked address, and we can swap the source without behavior
//! change. Then repeat per-target.

mod common;

use modforge::harness::{GameHarness, RunningGame};
use serde_json::{Value, json};

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
fn combinator_address_resolves_identically_via_sleuth() {
    if modforge::harness::should_skip() {
        eprintln!("MODFORGE_NO_GAME=1, skipping");
        return;
    }
    let spec = common::spec();
    let game =
        GameHarness::launch(&spec, "r2_migration_combinator").expect("harness launch failed");

    // Legacy path: hardcoded RVA -> runtime addr via targets::rebase
    // (exposed by the existing dryrun op).
    let legacy = dryrun_addr(&game, "genes.ext.combinator.dryrun", &["runtime_addr"]);

    // R2 path: derive a signature from the legacy address (32 bytes
    // of body, masking known wildcards), feed it to sleuth.
    let sig = fetch_signature_at(&game, &legacy, 32);

    // To prove the resolver isn't just memorizing the legacy
    // address, also test with the FIRST 4 BYTES wildcarded (still
    // a 32-byte signature; first 4 bytes won't match by
    // coincidence in unrelated code).
    let mut tokens: Vec<String> = sig.split_whitespace().map(str::to_string).collect();
    for t in tokens.iter_mut().take(4) {
        *t = "??".into();
    }
    let wildcarded = tokens.join(" ");

    let resp = game
        .op_json(
            "patterns.sleuth.resolve",
            &json!({
                "patterns": [
                    {"name": "GENE_COMBINATOR", "sigs": [&sig, &wildcarded]},
                ]
            }),
        )
        .expect("sleuth.resolve must succeed");

    let resolved = resp
        .get("result")
        .and_then(|r| r.get("results"))
        .and_then(|r| r.get("GENE_COMBINATOR"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    game.log().event(
        "R2-MIGRATE",
        &format!("legacy={legacy} resolved={resolved}"),
    );

    assert_eq!(
        resolved, legacy,
        "sleuth must resolve GENE_COMBINATOR to the SAME address as the hardcoded RVA"
    );
    game.pass("combinator legacy + sleuth paths converge");
}
