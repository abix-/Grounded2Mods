//! GENERIC: find which scene-table slot(s) in *(GS+0x438) hold a known
//! owned horse. User loads a save, optionally moves the horse between
//! containers (truck <-> pasture <-> race lane), runs this test, and
//! sees which slot the horse currently appears in.
//!
//! Designed to run with MODFORGE_ATTACH=1 against an already-running
//! game so we don't lose the in-game state by relaunching.
//!
//! Modes (env-driven):
//! - `HORSEY_HORSE=<name>`: target horse (default: first owned).
//! - default: discovery. Lists every (slot, idx, vec_count) where the
//!   horse appears. Asserts at least one hit.
//! - `HORSEY_EXPECT_SLOT=<hex>`: strict. Assert the horse appears in
//!   the named slot. Use to lock "owned horses are always in slot 0x00"
//!   or any other container-specific contract once observed.

mod common;

use serde_json::{Value, json};

#[test]
fn find_horse_slot() {
    let Some(game) = common::launch("hk1_find_tomtato_slot") else {
        return;
    };
    let want_name = std::env::var("HORSEY_HORSE").ok().filter(|s| !s.is_empty());
    let horses = common::list_owned(&game);

    let (name, ptr_s) = match want_name {
        Some(n) => {
            let target = horses.iter().find_map(|h| {
                let nm = h.get("name").and_then(Value::as_str)?;
                if nm.eq_ignore_ascii_case(&n) {
                    let p = h.get("ptr").and_then(Value::as_str)?;
                    Some((nm.to_string(), p.to_string()))
                } else {
                    None
                }
            });
            target.unwrap_or_else(|| {
                let names: Vec<_> = horses
                    .iter()
                    .filter_map(|h| h.get("name").and_then(Value::as_str).map(String::from))
                    .collect();
                panic!("HORSEY_HORSE='{n}' not owned. owned={names:?}");
            })
        }
        None => {
            let first = horses
                .first()
                .unwrap_or_else(|| panic!("no owned horses; load a save first"));
            let nm = first
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("?")
                .to_string();
            let p = first
                .get("ptr")
                .and_then(Value::as_str)
                .unwrap_or("0x0")
                .to_string();
            (nm, p)
        }
    };
    let target_ptr = u64::from_str_radix(ptr_s.trim_start_matches("0x"), 16).expect("bad ptr");
    eprintln!("\nlooking for {name} at {ptr_s} ({target_ptr:#x})");

    let resp = game
        .op_json("hk1.probe.scene_slot_vtables", &json!({}))
        .expect("scene slot op");
    let result = resp.get("result").unwrap_or(&resp);
    let arr_ptr_s = result
        .get("arr_ptr")
        .and_then(Value::as_str)
        .unwrap_or("0x0");
    let image_base_s = result
        .get("image_base")
        .and_then(Value::as_str)
        .unwrap_or("0x0");
    eprintln!("arr_ptr = {arr_ptr_s}, image_base = {image_base_s}");
    let slots = result
        .get("slots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut hits = Vec::new();
    for s in &slots {
        let slot_s = s.get("slot").and_then(Value::as_str).unwrap_or("?");
        let sub_ptr_s = s.get("sub_ptr").and_then(Value::as_str).unwrap_or("0x0");
        let count = s.get("count").and_then(Value::as_u64);
        let Some(count) = count else { continue };
        let sub_ptr = u64::from_str_radix(sub_ptr_s.trim_start_matches("0x"), 16).unwrap_or(0);
        let vec_begin_addr = sub_ptr + 0x130;
        let begin_resp = game
            .op_json("mem.peek", &json!({"addr": vec_begin_addr, "kind": "u64"}))
            .expect("mem.peek begin");
        let begin_s = begin_resp
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(Value::as_str)
            .unwrap_or("0x0");
        let vec_begin = u64::from_str_radix(begin_s.trim_start_matches("0x"), 16).unwrap_or(0);
        if vec_begin == 0 {
            continue;
        }
        for i in 0..count {
            let p_addr = vec_begin + i * 8;
            let p_resp = game
                .op_json("mem.peek", &json!({"addr": p_addr, "kind": "u64"}))
                .expect("mem.peek elem");
            let p_s = p_resp
                .get("result")
                .and_then(|r| r.get("value"))
                .and_then(Value::as_str)
                .unwrap_or("0x0");
            let p = u64::from_str_radix(p_s.trim_start_matches("0x"), 16).unwrap_or(0);
            if p == target_ptr {
                hits.push((slot_s.to_string(), i, count));
            }
        }
    }
    eprintln!("\n{name} at {ptr_s} found in slots:");
    for (slot, idx, n) in &hits {
        eprintln!("  slot {slot} idx {idx} (vec_count={n})");
    }
    assert!(
        !hits.is_empty(),
        "{name} not found in any slot; not owned or in an unscanned slot"
    );

    if let Ok(want_slot) = std::env::var("HORSEY_EXPECT_SLOT") {
        let norm = want_slot.trim_start_matches("0x").to_ascii_lowercase();
        let hit = hits
            .iter()
            .any(|(slot, _, _)| slot.trim_start_matches("0x").to_ascii_lowercase() == norm);
        assert!(
            hit,
            "HORSEY_EXPECT_SLOT={want_slot} but {name} was found in slots: {:?}",
            hits.iter().map(|(s, _, _)| s.clone()).collect::<Vec<_>>()
        );
    }

    game.pass(&format!("{name} found in {} slot(s)", hits.len()));
}
