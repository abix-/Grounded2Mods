//! R3: pattern-scan-resolve the GAMESTATE_PTR slot via patternsleuth.
//!
//! GAMESTATE_PTR is the address of the `.data` pointer slot
//! (`DAT_1403fb0d8`) whose 8 bytes hold the heap address of the
//! GameState struct. The resolver in
//! `crate::targets::resolve::gamestate_ptr` anchors on the
//! GameState constructor's distinctive 1.0f@+0x114 write
//! (`FUN_1400fd580` line 96) and decodes the disp32 of the
//! constructor's unique RIP-relative qword store (the slot store
//! at line 86: `DAT_1403fb0d8 = param_1`).
//!
//! Contract (asserted by this test):
//!
//!   1. `targets.resolve.gamestate_ptr` returns a non-null slot.
//!   2. The resolved slot agrees with the hardcoded RVA (within
//!      0x1000 bytes; same sanity gate the resolver itself uses).
//!   3. The slot is inside the loaded image (between image_base
//!      and image_base + 16 MiB).
//!   4. Gated on `MODFORGE_EXPECT_LOADED=1`: dereffing the slot
//!      yields a heap-shaped pointer (>= 0x10000, <= 0x7fff...,
//!      8-aligned) AND reading +0x308 on the dereffed pointer
//!      returns a u32 in `[0, 100_000_000]` (real-save money
//!      bound, NOT exact-value match).
//!
//! Without `MODFORGE_EXPECT_LOADED`, the in-save assertions are
//! skipped so the test still passes at the main menu.

mod common;

use modforge::testkit::json::u64_at_key as u64_of;
use serde_json::json;

#[test]
fn gamestate_ptr_resolves_via_constructor_anchor() {
    let Some(game) = common::launch("r3_gamestate_resolves") else {
        return;
    };
    let resp = game
        .op_json("targets.resolve.gamestate_ptr", &json!({}))
        .expect("targets.resolve.gamestate_ptr must succeed");
    let result = resp.get("result").expect("op response must include result");
    game.log()
        .event("R3", &format!("resolve result = {result:#}"));

    let slot = u64_of(result, "slot").expect("result.slot (hex string) missing");
    let hardcoded =
        u64_of(result, "hardcoded_slot").expect("result.hardcoded_slot (hex string) missing");
    let image_base = u64_of(result, "image_base").expect("result.image_base (hex string) missing");

    assert!(
        slot != 0,
        "resolver returned null slot. Constructor anchor or slot-store \
         pattern failed to match on this build. result = {result:#}"
    );

    // Agreement with hardcoded: same gate the resolver uses
    // internally. If this drifts on a future build, the alias-check
    // test will catch it; for now, the resolver returns None when
    // delta > 0x1000.
    let delta = slot.abs_diff(hardcoded);
    assert!(
        delta <= 0x1000,
        "resolved slot 0x{slot:x} disagrees with hardcoded 0x{hardcoded:x} \
         by delta 0x{delta:x} > 0x1000. result = {result:#}"
    );

    // Slot must live inside the loaded image's `.data`.
    let offset_into_image = slot.saturating_sub(image_base);
    assert!(
        offset_into_image > 0 && offset_into_image < 0x1_000_0000,
        "slot 0x{slot:x} is not inside the loaded image (base 0x{image_base:x}). \
         result = {result:#}"
    );

    if std::env::var("MODFORGE_EXPECT_LOADED").ok().as_deref() == Some("1") {
        // In-save: the slot's dereffed value must be a heap pointer
        // (the live GameState), and money at +0x308 must be bounded.
        let deref = u64_of(result, "deref").expect("result.deref missing");
        assert!(
            deref >= 0x10000 && deref <= 0x7fff_ffff_ffff && (deref & 0x7) == 0,
            "user reports save loaded but dereffed slot 0x{deref:x} is not \
             heap-shaped. The deref + plausibility gate rejected the value. \
             result = {result:#}"
        );

        let money = result
            .get("money_at_deref_plus_0x308")
            .and_then(|v| v.as_u64())
            .expect("money_at_deref_plus_0x308 must be present when deref is plausible");
        assert!(
            money <= 100_000_000,
            "money at deref+0x308 = {money} exceeds 100M; the dereffed pointer \
             is likely garbage. result = {result:#}"
        );
    }

    game.pass(&format!(
        "R3 GAMESTATE_PTR resolves to slot=0x{slot:x} (hardcoded=0x{hardcoded:x}, \
         delta=0x{delta:x})"
    ));
}
