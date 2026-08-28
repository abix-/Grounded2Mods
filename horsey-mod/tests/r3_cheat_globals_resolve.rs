//! R3 batch 1: pattern-scan-resolve the cheat globals AND validate
//! every resolved address against the hardcoded one via a memory-
//! aliasing probe.
//!
//! Each global is a single byte in `.data`. The resolver finds it
//! via a sleuth signature for an instruction that references it.
//! Validation: write a sentinel at the hardcoded address and read
//! back via the resolved address. If they're the same byte the
//! sentinel round-trips. The probe is atomic (writes, reads,
//! restores) and uses TWO distinct sentinel values (0xAB then 0xCD)
//! so we catch both fixed-value and partial-aliasing false-positives.
//!
//! Contract:
//!   1. `targets.resolve.cheat_globals` returns the resolved
//!      address + the hardcoded RVA for each global.
//!   2. If the resolver returns a non-null address, `mem.alias_check`
//!      between hardcoded and resolved MUST report `same_byte: true`.
//!      Mismatch means hardcoded and resolved point at DIFFERENT
//!      bytes; one of them is wrong. Test fails loud with both
//!      addresses + the read-back values so the user can diff.
//!   3. If the resolver returned null (sig didn't match this build),
//!      the global is logged as "still hardcoded" and skipped.
//!
//! The test does not modify game-visible state: the probe writes
//! 0xAB / 0xCD transiently, then restores the byte to its original
//! value. The window is ~3 atomic byte writes; if a game frame races
//! during that window the cheat flag might flicker for one tick.

mod common;

use modforge::testkit::json::u64_at_key as u64_of;
use serde_json::{Value, json};

fn validate(game: &modforge::harness::RunningGame, name: &str, entry: &Value) -> bool {
    let resolved = u64_of(entry, "address").unwrap_or(0);
    let hardcoded = u64_of(entry, "hardcoded").unwrap_or(0);
    if resolved == 0 {
        game.log().event(
            "R3-CHEATS",
            &format!("{name}: sig did not match, leaving hardcoded ({hardcoded:#x})"),
        );
        return false;
    }
    if resolved == hardcoded {
        game.log().event(
            "R3-CHEATS",
            &format!("{name}: resolved == hardcoded ({resolved:#x}), trivially aliased"),
        );
        return true;
    }
    // Different addresses; run the aliasing probe.
    let resp = game
        .op_json(
            "mem.alias_check",
            &json!({
                "addr_a": hardcoded,
                "addr_b": resolved,
            }),
        )
        .expect("mem.alias_check must succeed");
    let probe = resp.get("result").expect("result");
    let same = probe
        .get("same_byte")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    game.log().event(
        "R3-CHEATS",
        &format!("{name}: hardcoded={hardcoded:#x} resolved={resolved:#x} probe={probe:#}"),
    );
    assert!(
        same,
        "{name}: resolved address {resolved:#x} does NOT alias the hardcoded \
         address {hardcoded:#x}. Probe: {probe:#}. One of them is wrong for \
         this build."
    );
    true
}

#[test]
fn cheat_globals_resolve_and_alias_hardcoded() {
    let Some(game) = common::launch("r3_cheat_globals") else {
        return;
    };
    let resp = game
        .op_json("targets.resolve.cheat_globals", &json!({}))
        .expect("targets.resolve.cheat_globals must succeed");
    let result = resp.get("result").expect("op response must include result");
    game.log()
        .event("R3-CHEATS", &format!("resolve = {result:#}"));

    let mut validated = 0;
    for name in ["no_tire_toggle", "debug_mode_active", "debug_log_gate"] {
        let entry = result.get(name).expect("entry");
        if validate(&game, name, entry) {
            validated += 1;
        }
    }
    game.pass(&format!(
        "R3 cheat globals: {validated}/3 resolved + aliased the hardcoded byte"
    ));
}
