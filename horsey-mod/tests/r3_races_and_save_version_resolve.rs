//! Verify RACES_COUNTER + SAVE_VERSION_GLOBAL resolve correctly.
//!
//! RACES_COUNTER is recovered from the unique `-10.5f` (0xc1280000)
//! float-init that precedes the RACES_COUNTER zero-init in the
//! decomp at all_functions.c:84919-84920.
//!
//! SAVE_VERSION_GLOBAL is structurally at GAMESTATE_PTR + 8 (the
//! version slot lives 8 bytes after the GameState pointer slot in
//! `.data`).
//!
//! Contract:
//!   - Both resolvers return non-null.
//!   - Resolved addresses are within 0x1000 of the rebased
//!     hardcoded RVA (the same sanity gate the resolvers use).

mod common;

use modforge::testkit::json::u64_at_key as u64_of;
use serde_json::json;

#[test]
fn data_globals_resolve_within_sanity_band() {
    let Some(game) = common::launch("r3_races_and_save_version_resolve") else {
        return;
    };
    let resp = game
        .op_json("targets.resolve.data_globals", &json!({}))
        .expect("targets.resolve.data_globals must succeed");
    let result = resp.get("result").expect("result");
    game.log()
        .event("R3-DATA", &format!("data_globals_resp = {result:#}"));

    for name in &["races_counter", "save_version_global"] {
        let entry = result
            .get(*name)
            .unwrap_or_else(|| panic!("response missing {name}: {result}"));
        let resolved = u64_of(entry, "address").unwrap_or(0);
        let hardcoded = u64_of(entry, "hardcoded").unwrap_or(0);
        assert!(
            resolved != 0,
            "{name}: resolver returned null. result = {result:#}"
        );
        assert!(
            hardcoded != 0,
            "{name}: missing hardcoded; result = {result:#}"
        );
        let delta = resolved.abs_diff(hardcoded);
        assert!(
            delta <= 0x1000,
            "{name}: resolved 0x{resolved:x} disagrees with hardcoded 0x{hardcoded:x} \
             by 0x{delta:x} > 0x1000"
        );
        game.log().event(
            "R3-DATA",
            &format!("{name}: 0x{resolved:x} (hardcoded=0x{hardcoded:x}, delta=0x{delta:x})"),
        );
    }
    game.pass("RACES_COUNTER + SAVE_VERSION_GLOBAL resolve within sanity band");
}
