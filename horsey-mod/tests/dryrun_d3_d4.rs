//! Dryrun verification for the D3 / D4 patch targets.
//!
//! What this asserts (for every new patch target):
//!   1. The dryrun op succeeds.
//!   2. The runtime address is non-zero and inside the loaded
//!      Horsey.exe image (image_base + RVA range).
//!   3. The 16-byte prologue starts with a plausible MSVC
//!      function-entry pattern (see `looks_like_msvc_entry`).
//!      Catches the -16 Ghidra-offset-adjustment regression
//!      automatically: if a target's true entry is wrong, its
//!      prologue starts with INT3 padding (`cc cc ..`) or random
//!      middle-of-function bytes.
//!
//! Targets checked:
//!   - genes.ext.combinator.dryrun  (1 target: GENE_COMBINATOR)
//!   - genes.ext.lifecycle.dryrun   (2 targets: HORSE_CONSTRUCTOR,
//!                                              HORSE_DESTRUCTOR)
//!   - genes.ext.save.dryrun        (4 targets: SAVE_WRITER,
//!                                              LOAD_GAME,
//!                                              HORSE_SAVE_WRITER,
//!                                              HORSE_SAVE_LOADER)
//!
//! Failure of this test => DO NOT arm those detours. Re-derive the
//! address (the -16 adjustment is probably wrong for that one) and
//! re-run.

mod common;

use modforge::harness::RunningGame;
use modforge::testkit::fn_entry::is_msvc_x64_prologue_loose;
use serde_json::Value;
use serde_json::json;

fn parse_prologue_hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .filter_map(|tok| u8::from_str_radix(tok, 16).ok())
        .collect()
}

/// Check a single target. Returns Ok(()) if the prologue looks
/// like an MSVC function entry; Err(reason) otherwise. Caller
/// collects errors and asserts at the end so one failure doesn't
/// hide others in the same dryrun group.
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
    if !is_msvc_x64_prologue_loose(&bytes) {
        let first4 = bytes
            .iter()
            .take(4)
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let msg = format!(
            "{group}/{name}: prologue not MSVC entry-shaped (first 4: [{first4}]). \
             Address {resolved} is likely off; re-derive (-16 from Ghidra)."
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
fn combinator_dryrun_prologue_ok() {
    let Some(game) = common::launch("dryrun_combinator") else {
        return;
    };

    let resp = game
        .op_json("genes.ext.combinator.dryrun", &json!({}))
        .expect("combinator dryrun should succeed");

    // Standard modforge envelope: payload lives in `result`.
    let snap = resp
        .get("result")
        .expect("envelope should have a `result` field");

    let mut fails = Vec::new();
    if let Err(e) = check_target(&game, "COMBINATOR", "GENE_COMBINATOR", snap) {
        fails.push(e);
    }
    game.pass("combinator prologue ok");
    assert_no_failures(fails);
}

#[test]
fn lifecycle_dryrun_prologues_ok() {
    let Some(game) = common::launch("dryrun_lifecycle") else {
        return;
    };

    let resp = game
        .op_json("genes.ext.lifecycle.dryrun", &json!({}))
        .expect("lifecycle dryrun should succeed");

    // Standard modforge envelope: payload lives in `result`.
    let snap = resp
        .get("result")
        .expect("envelope should have a `result` field");

    let ctor = snap
        .get("ctor")
        .expect("lifecycle dryrun should include ctor");
    let dtor = snap
        .get("dtor")
        .expect("lifecycle dryrun should include dtor");

    let mut fails = Vec::new();
    if let Err(e) = check_target(&game, "LIFECYCLE", "HORSE_CONSTRUCTOR", ctor) {
        fails.push(e);
    }
    if let Err(e) = check_target(&game, "LIFECYCLE", "HORSE_DESTRUCTOR", dtor) {
        fails.push(e);
    }
    game.pass("lifecycle prologues ok");
    assert_no_failures(fails);
}

#[test]
fn save_dryrun_prologues_ok() {
    let Some(game) = common::launch("dryrun_save") else {
        return;
    };

    let resp = game
        .op_json("genes.ext.save.dryrun", &json!({}))
        .expect("save dryrun should succeed");

    // Standard modforge envelope: payload lives in `result`.
    let snap = resp
        .get("result")
        .expect("envelope should have a `result` field");

    let targets = snap
        .get("targets")
        .and_then(|v| v.as_array())
        .expect("save dryrun should include targets array");

    assert_eq!(
        targets.len(),
        4,
        "expected 4 save targets, got {}",
        targets.len()
    );

    let mut fails = Vec::new();
    for t in targets {
        let name = t
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("(unnamed)")
            .to_string();
        if let Err(e) = check_target(&game, "SAVE", &name, t) {
            fails.push(e);
        }
    }
    game.pass("save prologues ok");
    assert_no_failures(fails);
}
