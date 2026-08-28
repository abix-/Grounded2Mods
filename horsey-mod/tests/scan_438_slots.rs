//! GENERIC: dump every (*(GS+0x438) + slot) +0x130/+0x138 horse-vector
//! count. The cmdlet `gamestate.scan_438_slots` is the resolver; this
//! test asserts the structure is healthy and (optionally) locks in the
//! slot that holds the owned-horse list.
//!
//! Modes (env-driven):
//! - default: discovery. Logs every slot, asserts at least one has a
//!   plausible horse-vector (count <= 50).
//! - `HORSEY_EXPECT_OWNED=<N>`: strict. Assert exactly one slot reports
//!   count == N (the owned-horse vector). Useful for regression
//!   coverage that the +0x438 sub-struct layout still walks correctly.
//! - `HORSEY_EXPECT_OWNED_SLOT=<hex>`: strict. Combined with EXPECT_OWNED,
//!   asserts the matching slot is the named one (e.g. `0x00`).

mod common;

use serde_json::{Value, json};

#[test]
fn dump_slot_horse_counts() {
    let Some(game) = common::launch("scan_438_slots") else {
        return;
    };
    let expected_owned: Option<u64> = std::env::var("HORSEY_EXPECT_OWNED")
        .ok()
        .map(|s| s.parse().expect("HORSEY_EXPECT_OWNED not a number"));
    let expected_slot: Option<String> = std::env::var("HORSEY_EXPECT_OWNED_SLOT").ok();

    let resp = game
        .op_json("gamestate.scan_438_slots", &json!({}))
        .expect("scan_438_slots op should succeed");
    eprintln!("\nfull response: {resp}");
    let result = resp.get("result").unwrap_or(&resp);
    eprintln!("\narr_ptr: {:?}", result.get("arr_ptr"));
    let slots = result
        .get("slots_with_horse_vec")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    eprintln!("slots_with_horse_vec ({} entries):", slots.len());
    for s in &slots {
        eprintln!("  {s}");
    }

    let plausible: Vec<&Value> = slots
        .iter()
        .filter(|s| {
            s.get("count")
                .and_then(Value::as_u64)
                .is_some_and(|c| c <= 50)
        })
        .collect();
    assert!(
        !plausible.is_empty(),
        "no slot in *(GS+0x438) has a plausible horse-vector (count <= 50); structure broken"
    );

    if let Some(want) = expected_owned {
        let matches: Vec<&&Value> = plausible
            .iter()
            .filter(|s| s.get("count").and_then(Value::as_u64) == Some(want))
            .collect();
        assert!(
            !matches.is_empty(),
            "HORSEY_EXPECT_OWNED={want} but no slot reports that count. plausible={plausible:?}"
        );
        if let Some(want_slot) = &expected_slot {
            let want_slot_norm = want_slot.trim_start_matches("0x").to_ascii_lowercase();
            let hit = matches.iter().any(|s| {
                let slot = s.get("slot").and_then(Value::as_str).unwrap_or("");
                slot.trim_start_matches("0x").to_ascii_lowercase() == want_slot_norm
            });
            assert!(
                hit,
                "HORSEY_EXPECT_OWNED_SLOT={want_slot} but slot with count={want} was not at that slot. matches={matches:?}"
            );
        }
    }

    game.pass(&format!(
        "scanned {} slots, {} plausible",
        slots.len(),
        plausible.len()
    ));
}
