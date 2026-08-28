//! R4: every entry returned by `targets.resolve.field_offsets`
//! must have `resolved == hardcoded` on the current Horsey build.
//!
//! Table-driven from the op response itself, so adding a new
//! resolver in `ops.rs` automatically extends coverage without
//! touching this test. Each mismatch is logged per-entry and the
//! full set is reported at end so a single bad resolver doesn't
//! mask the rest.

mod common;

use serde_json::{Value, json};

fn parse_hex(v: &Value) -> Option<u64> {
    v.as_str()
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
}

#[test]
fn all_field_offsets_resolved_match_hardcoded() {
    let Some(game) = common::launch("r4_field_offsets") else {
        return;
    };

    let resp = game
        .op_json("targets.resolve.field_offsets", &json!({}))
        .expect("targets.resolve.field_offsets must succeed");
    let result = resp
        .get("result")
        .and_then(|v| v.as_object())
        .expect("result must be an object");

    assert!(!result.is_empty(), "result has no field-offset entries");

    let mut failures: Vec<String> = Vec::new();
    let mut keys: Vec<&String> = result.keys().collect();
    keys.sort();

    for key in keys {
        let entry = result
            .get(key)
            .and_then(|v| v.as_object())
            .unwrap_or_else(|| panic!("entry {key} is not an object"));
        let resolved = entry
            .get("resolved")
            .and_then(parse_hex)
            .unwrap_or_else(|| panic!("entry {key} missing resolved hex"));
        let hardcoded = entry
            .get("hardcoded")
            .and_then(parse_hex)
            .unwrap_or_else(|| panic!("entry {key} missing hardcoded hex"));

        game.log().event(
            "R4",
            &format!("{key}: resolved=0x{resolved:x} hardcoded=0x{hardcoded:x}"),
        );

        if resolved != hardcoded {
            failures.push(format!(
                "{key}: resolved=0x{resolved:x} hardcoded=0x{hardcoded:x}"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "R4 resolver mismatches ({} of {}):\n  {}",
        failures.len(),
        result.len(),
        failures.join("\n  "),
    );

    game.pass(&format!(
        "R4: {} field offsets resolved, all match hardcoded",
        result.len()
    ));
}
