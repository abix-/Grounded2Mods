//! GENERIC: per scene-table slot in `*(GS+0x438)`, report vtable RVA +
//! horse-vec count. Classifies slots into subsystem groups.
//!
//! Modes (env-driven):
//! - default: discovery. Logs all slots, asserts at least one has a
//!   plausible horse vector (count <= 20).
//! - `HORSEY_EXPECT_VTABLE_RVA=<hex>`: assert at least one slot has
//!   this vtable RVA. Use to regression-protect a known scene class
//!   address (e.g. HLT's `kNeighborSceneVtableRva = 0x3037D0` or
//!   `kWildHorseVtableRva = 0x310C90`).
//! - `HORSEY_EXPECT_VTABLE_SLOT=<hex>`: combined with EXPECT_VTABLE_RVA,
//!   assert the matching vtable is at that slot offset.

mod common;

use serde_json::{Value, json};

#[test]
fn probe_scene_slot_vtables() {
    let Some(game) = common::launch("hk1_probe_scene_slot_vtables") else {
        return;
    };
    eprintln!("[GATE] waiting for a save to load (up to 120s) ...");
    common::wait_for_target_horse(&game, std::time::Duration::from_secs(120));

    let expect_vtable: Option<String> = std::env::var("HORSEY_EXPECT_VTABLE_RVA").ok();
    let expect_slot: Option<String> = std::env::var("HORSEY_EXPECT_VTABLE_SLOT").ok();

    let resp = game
        .op_json("hk1.probe.scene_slot_vtables", &json!({}))
        .expect("hk1.probe.scene_slot_vtables op should succeed");
    let result = resp.get("result").unwrap_or(&resp);
    let slots = result
        .get("slots")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let image_base = result
        .get("image_base")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    eprintln!("\nimage_base = {image_base}");
    eprintln!("slots ({}):", slots.len());
    for s in &slots {
        eprintln!("  {s}");
    }
    assert!(!slots.is_empty(), "no scene-table slots found at all");

    let any_with_horses = slots.iter().any(|s| {
        s.get("count")
            .and_then(|v| v.as_u64())
            .is_some_and(|c| c <= 20)
    });
    assert!(any_with_horses, "no slot has a plausible horse vector");

    if let Some(want_rva) = &expect_vtable {
        let want_norm = want_rva.trim_start_matches("0x").to_ascii_lowercase();
        let matches: Vec<&Value> = slots
            .iter()
            .filter(|s| {
                s.get("vtable_rva")
                    .and_then(Value::as_str)
                    .map(|v| v.trim_start_matches("0x").to_ascii_lowercase() == want_norm)
                    .unwrap_or(false)
            })
            .collect();
        assert!(
            !matches.is_empty(),
            "HORSEY_EXPECT_VTABLE_RVA={want_rva} not found in any slot. \
             vtables seen: {:?}",
            slots
                .iter()
                .filter_map(|s| s.get("vtable_rva").and_then(Value::as_str))
                .collect::<Vec<_>>()
        );
        if let Some(want_slot) = &expect_slot {
            let slot_norm = want_slot.trim_start_matches("0x").to_ascii_lowercase();
            let hit = matches.iter().any(|s| {
                s.get("slot")
                    .and_then(Value::as_str)
                    .map(|v| v.trim_start_matches("0x").to_ascii_lowercase() == slot_norm)
                    .unwrap_or(false)
            });
            assert!(
                hit,
                "HORSEY_EXPECT_VTABLE_SLOT={want_slot} expected to hold vtable {want_rva}, \
                 but it was found at other slot(s): {matches:?}"
            );
        }
    }

    game.pass(&format!("classified {} scene-table slots", slots.len()));
}
