//! Dryrun verification for the already-shipped D1 + D5 patch targets.
//!
//! These are the detours that have been working in-game for weeks
//! (slot 0 visual proof, etc.). We trust them empirically; this test
//! adds an automated regression fence so a future build that changes
//! prologue layout doesn't break them silently.
//!
//! Covered:
//!   - genes.ext.dryrun         (D1: EVAL_DIPLOID_BLEND_A,
//!                                   EVAL_DIPLOID_BLEND_B,
//!                                   GENE_ALLELE_SWAP,
//!                                   GENE_DEATH_DRIFT [unwired])
//!   - genes.ext.render.dryrun  (D5: APPLY_GENE_TO_HORSE)
//!
//! All entries assert an MSVC function-entry shape via the same
//! classifier used in `dryrun_d3_d4.rs`. Unwired targets are still
//! checked (because the `dryrun` op reports them).

mod common;

use modforge::harness::RunningGame;
use modforge::testkit::fn_entry::is_msvc_x64_prologue_loose as looks_like_msvc_entry;
use serde_json::{Value, json};

fn parse_prologue_hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .filter_map(|tok| u8::from_str_radix(tok, 16).ok())
        .collect()
}

fn check_target(game: &RunningGame, group: &str, name: &str, target: &Value) -> Result<(), String> {
    let resolved = target
        .get("runtime_addr")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let prologue_str = target
        .get("prologue_bytes")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let bytes = parse_prologue_hex(prologue_str);

    let log = game.log();
    log.event(
        group,
        &format!("{name} resolved={resolved} prologue=[{prologue_str}]"),
    );

    if resolved.is_empty() || resolved == "0x0" {
        let msg = format!("{group}/{name}: empty/zero runtime_addr");
        log.event(group, &format!("{name} FAIL: {msg}"));
        return Err(msg);
    }
    if bytes.len() < 8 {
        let msg = format!("{group}/{name}: prologue too short ({} bytes)", bytes.len());
        log.event(group, &format!("{name} FAIL: {msg}"));
        return Err(msg);
    }
    if !looks_like_msvc_entry(&bytes) {
        let first4 = bytes
            .iter()
            .take(4)
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let msg = format!(
            "{group}/{name}: prologue not MSVC entry-shaped (first 4: [{first4}]). \
             Address {resolved} may need re-derivation."
        );
        log.event(group, &format!("{name} FAIL: {msg}"));
        return Err(msg);
    }
    log.event(group, &format!("{name} PROLOGUE OK"));
    Ok(())
}

fn assert_no_failures(failures: Vec<String>) {
    if !failures.is_empty() {
        let combined = failures.join("\n  - ");
        panic!("prologue check failed:\n  - {combined}");
    }
}

#[test]
fn d1_dryrun_prologues_ok() {
    let Some(game) = common::launch("dryrun_d1") else {
        return;
    };

    let resp = game
        .op_json("genes.ext.dryrun", &json!({}))
        .expect("D1 dryrun should succeed");

    let snap = resp
        .get("result")
        .expect("envelope should have a `result` field");

    let targets = snap
        .get("targets")
        .and_then(|v| v.as_array())
        .expect("D1 dryrun should include targets array");

    assert!(
        !targets.is_empty(),
        "D1 dryrun returned zero targets; targets table is empty"
    );

    let mut fails = Vec::new();
    for t in targets {
        let name = t
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("(unnamed)")
            .to_string();
        if let Err(e) = check_target(&game, "D1", &name, t) {
            fails.push(e);
        }
    }
    game.pass("d1 prologues ok");
    assert_no_failures(fails);
}

#[test]
fn d5_render_dryrun_prologue_ok() {
    let Some(game) = common::launch("dryrun_d5_render") else {
        return;
    };

    let resp = game
        .op_json("genes.ext.render.dryrun", &json!({}))
        .expect("D5 render dryrun should succeed");

    // render dryrun puts the target fields directly in `result`,
    // not nested under a `targets` array. Pass `result` itself to
    // check_target.
    let snap = resp
        .get("result")
        .expect("envelope should have a `result` field");

    let mut fails = Vec::new();
    if let Err(e) = check_target(&game, "D5", "APPLY_GENE_TO_HORSE", snap) {
        fails.push(e);
    }
    game.pass("d5 prologue ok");
    assert_no_failures(fails);
}
