//! Diagnostic / re-derivation probe for GAMESTATE_PTR.
//!
//! When a Horsey build drifts, the world-root pointer slot moves and
//! `targets.resolve.gamestate_ptr` falls back to the stale hardcoded
//! RVA (fingerprint: `slot == hardcoded_slot` with a null/garbage
//! deref). This test scans the CURRENT binary for the world-root store
//! pattern `48 89 1D ?? ?? ?? ?? 48 89 BB 70 02 00 00`, decodes the
//! rel32 to the live slot, derefs it, and structurally validates the
//! candidate as the real GameState (active_scene_id in [-1,256),
//! scene_table at +0x438 heap-shaped). It PRINTS the new slot RVA to
//! bake into `targets_registry::GAMESTATE_PTR.hint_rva` and
//! `targets::GAMESTATE_PTR`.
//!
//! Not an assertion gate; it is the re-derivation instrument (RULE 3:
//! probes ship as tests). Fresh launch by the harness; the save
//! auto-loads so gamestate is live by the time HTTP comes up.

mod common;

use serde_json::{json, Value};

/// Parse the first `n` bytes of a `"e8 c1 .. .."` hex string as a
/// little-endian unsigned integer (how Horsey stores pointers/ints).
fn le_from_hex(hex: &str, n: usize) -> u64 {
    let bytes: Vec<u8> = hex
        .split_whitespace()
        .filter_map(|b| u8::from_str_radix(b, 16).ok())
        .collect();
    let mut v = 0u64;
    for (i, b) in bytes.iter().take(n).enumerate() {
        v |= (*b as u64) << (8 * i);
    }
    v
}

fn read_le(game: &modforge::harness::RunningGame, addr_va: u64, n: usize) -> Option<u64> {
    let v = game
        .op_json("patterns.read_bytes", &json!({"addr": format!("0x{addr_va:x}"), "n": n}))
        .ok()?;
    let r = v.get("result").unwrap_or(&v);
    let hex = r.get("bytes").and_then(Value::as_str)?;
    Some(le_from_hex(hex, n))
}

fn heap_shaped(p: u64) -> bool {
    p > 0x1_0000 && p < 0x7fff_ffff_ffff
}

#[test]
fn rederive_gamestate_ptr() {
    let Some(game) = common::launch("rederive_gamestate_ptr") else { return };

    // --- 1. Binary identity (did Steam patch the game?) ---
    match game.op_json("game.build_info", &json!({})) {
        Ok(info) => eprintln!("[BUILD_INFO] {}", serde_json::to_string_pretty(&info).unwrap()),
        Err(e) => eprintln!("[BUILD_INFO] op failed: {e}"),
    }

    // --- 2. Current (suspected-broken) resolver output + image base ---
    let res = game
        .op_json("targets.resolve.gamestate_ptr", &json!({}))
        .expect("resolve op");
    let r = res.get("result").unwrap_or(&res);
    eprintln!("[RESOLVE current] {}", serde_json::to_string_pretty(r).unwrap());
    let image_base = u64::from_str_radix(
        r.get("image_base").and_then(Value::as_str).unwrap_or("0x0").trim_start_matches("0x"),
        16,
    )
    .unwrap_or(0);
    eprintln!("[IMAGE_BASE] 0x{image_base:x}");

    // --- 3. Scan the world-root store pattern; decode each slot ---
    let scan = game
        .op_json(
            "patterns.sleuth.scan_all",
            &json!({
                "sig": "48 89 1D ?? ?? ?? ?? 48 89 BB 70 02 00 00",
                "disp32_offset": 3,
                "instr_len": 7,
                "context_bytes": 24
            }),
        )
        .expect("scan_all op");
    let s = scan.get("result").unwrap_or(&scan);
    let hits = s.get("hits").and_then(Value::as_array).cloned().unwrap_or_default();
    eprintln!("[SCAN] {} hit(s) for the world-root store sig", hits.len());

    if hits.is_empty() {
        eprintln!(
            "[VERDICT] pattern no longer matches -- the surrounding bytes changed in this build. \
             Need a fresh anchor (constructor 1.0f@+0x114 store, or loosen the follow-up)."
        );
        return;
    }

    let mut winners: Vec<u64> = Vec::new();
    for (i, h) in hits.iter().enumerate() {
        let instr = h.get("instr_addr").and_then(Value::as_str).unwrap_or("?");
        let target = h.get("decoded_target").and_then(Value::as_str).unwrap_or("0x0");
        let ctx = h.get("context_hex").and_then(Value::as_str).unwrap_or("");
        let slot_va = u64::from_str_radix(target.trim_start_matches("0x"), 16).unwrap_or(0);
        let slot_rva = slot_va.wrapping_sub(image_base);
        eprintln!("  hit[{i}] instr={instr}");
        eprintln!("         slot_va={target}  slot_rva=0x{slot_rva:x}");
        eprintln!("         ctx: {ctx}");

        let Some(gs) = read_le(&game, slot_va, 8) else {
            eprintln!("         deref unreadable");
            continue;
        };
        eprintln!("         deref(slot) = gamestate_ptr = 0x{gs:x}");
        if !heap_shaped(gs) {
            eprintln!("         -> deref not heap-shaped; reject");
            continue;
        }
        let asid = read_le(&game, gs + 0x25C, 4).map(|v| v as i32);
        let scene_table = read_le(&game, gs + 0x438, 8);
        let asid_ok = matches!(asid, Some(a) if (-1..256).contains(&a));
        let st_ok = scene_table.map(heap_shaped).unwrap_or(false);
        eprintln!(
            "         active_scene_id(+0x25C) = {:?} (ok={asid_ok})",
            asid
        );
        eprintln!(
            "         scene_table(+0x438) = {} (heap-shaped={st_ok})",
            scene_table.map(|v| format!("0x{v:x}")).unwrap_or_else(|| "<unreadable>".into())
        );
        if asid_ok && st_ok {
            eprintln!("         -> STRUCTURALLY VALID. new GAMESTATE_PTR RVA = 0x{slot_rva:x}");
            winners.push(slot_rva);
        }
    }

    match winners.as_slice() {
        [rva] => eprintln!(
            "[VERDICT] unique structurally-valid slot. Bake into targets_registry::GAMESTATE_PTR.hint_rva \
             and targets::GAMESTATE_PTR: 0x1{rva:08x} (RVA 0x{rva:x})"
        ),
        [] => eprintln!("[VERDICT] pattern matched but no hit passed structural validation -- investigate."),
        many => eprintln!("[VERDICT] {} structurally-valid slots; need a tighter anchor: {many:x?}", many.len()),
    }
}
