//! Find what references the trailer horse (alpha) on the overworld.
//!
//! On the overworld alpha is in NO scene-table slot (see
//! locate_trailer_horse). Its Horse allocation still persists (session
//! pointer 0x170ef2c9f50; override with HORSEY_ALPHA_PTR). This scans the
//! GameState struct and the truck object (*(GS+0x300)), both as direct
//! fields and one level of vector indirection, for that pointer value,
//! to learn which container holds the trailer horse when off-scene.
//!
//! All reads go through SEH-guarded patterns.read_bytes, so following a
//! bad pointer returns an error instead of faulting the worker.

mod common;

use serde_json::{Value, json};

fn read_qwords(game: &modforge::harness::RunningGame, addr: u64, n: usize) -> Vec<u64> {
    let v = match game.op_json(
        "patterns.read_bytes",
        &json!({ "addr": format!("0x{addr:x}"), "n": n }),
    ) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let bytes_hex = v
        .get("result")
        .unwrap_or(&v)
        .get("bytes")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let bytes: Vec<u8> = bytes_hex
        .split_whitespace()
        .filter_map(|h| u8::from_str_radix(h, 16).ok())
        .collect();
    bytes
        .chunks_exact(8)
        .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

fn heapish(p: u64) -> bool {
    p > 0x1_0000 && p < 0x7fff_ffff_ffff
}

fn scan_region(
    game: &modforge::harness::RunningGame,
    label: &str,
    base: u64,
    n: usize,
    target: u64,
) {
    let qs = read_qwords(game, base, n);
    if qs.is_empty() {
        eprintln!("  {label} @ 0x{base:x}: unreadable");
        return;
    }
    // Direct hits.
    for (i, &q) in qs.iter().enumerate() {
        if q == target {
            eprintln!("  DIRECT: {label}+0x{:x} == alpha", i * 8);
        }
    }
    // One level of vector indirection: (begin,end) at adjacent qwords.
    for i in 0..qs.len().saturating_sub(1) {
        let (begin, end) = (qs[i], qs[i + 1]);
        if heapish(begin) && end > begin && (end - begin) % 8 == 0 && (end - begin) <= 0x800 {
            let inner = read_qwords(game, begin, (end - begin) as usize);
            if inner.contains(&target) {
                eprintln!(
                    "  VECTOR: {label}+0x{:x} -> [0x{begin:x}..0x{end:x}] ({} elems) CONTAINS alpha",
                    i * 8,
                    inner.len()
                );
            }
        }
    }
}

#[test]
fn find_alpha_ref() {
    let Some(game) = common::launch("find_alpha_ref") else {
        return;
    };

    let alpha = std::env::var("HORSEY_ALPHA_PTR")
        .ok()
        .and_then(|s| u64::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok())
        .unwrap_or(0x170ef2c9f50);

    eprintln!("active_scene_id = {:?}", common::active_scene_id(&game));
    eprintln!("hunting for alpha Horse* = 0x{alpha:x}");

    // Sanity: confirm alpha pointer still resolves to name_id 344.
    match game.op_json("horse.read", &json!({ "addr": format!("0x{alpha:x}") })) {
        Ok(v) => {
            let r = v.get("result").unwrap_or(&v);
            let nid = r.get("name_id").and_then(Value::as_u64);
            eprintln!("alpha sanity: name_id at that ptr = {nid:?} (expect 344)");
        }
        Err(e) => {
            eprintln!("alpha sanity read err: {e} (pointer may be stale; pass HORSEY_ALPHA_PTR)")
        }
    }

    let scan = game
        .op_json("gamestate.scan_438_slots", &json!({}))
        .expect("scan op");
    let r = scan.get("result").unwrap_or(&scan).clone();
    let gs = r
        .get("gs_ptr")
        .and_then(Value::as_str)
        .map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0))
        .unwrap_or(0);
    eprintln!("gs_ptr = 0x{gs:x}");
    if gs == 0 {
        eprintln!("no gamestate");
        return;
    }

    eprintln!("\n=== scan GameState struct ===");
    scan_region(&game, "GS", gs, 0x450, alpha);

    eprintln!("\n=== scan truck object (*(GS+0x300)) ===");
    let truck_q = read_qwords(&game, gs + 0x300, 8);
    let truck = truck_q.first().copied().unwrap_or(0);
    eprintln!("truck = 0x{truck:x}");
    if heapish(truck) {
        scan_region(&game, "truck", truck, 0x300, alpha);
    } else {
        eprintln!("  truck ptr not heap-shaped");
    }

    eprintln!("\n=== scan every scene-table slot's Location sub-struct ===");
    let slots = r
        .get("slots_with_horse_vec")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for s in &slots {
        let slot = s.get("slot").and_then(Value::as_str).unwrap_or("?");
        let sub = s
            .get("sub_ptr")
            .and_then(Value::as_str)
            .map(|x| u64::from_str_radix(x.trim_start_matches("0x"), 16).unwrap_or(0))
            .unwrap_or(0);
        if heapish(sub) {
            scan_region(&game, &format!("loc[{slot}]"), sub, 0x300, alpha);
        }
    }
    eprintln!("(only hits are printed above; silence = not found in any Location sub-struct)");

    eprintln!("\n=== confirm: Home Location (slot 0) horse vector triple ===");
    if let Some(s0) = slots
        .iter()
        .find(|s| s.get("slot").and_then(Value::as_str) == Some("0x0"))
    {
        let sub = s0
            .get("sub_ptr")
            .and_then(Value::as_str)
            .map(|x| u64::from_str_radix(x.trim_start_matches("0x"), 16).unwrap_or(0))
            .unwrap_or(0);
        let trip = read_qwords(&game, sub + 0x130, 0x18); // begin, end, capacity
        if trip.len() == 3 {
            let (begin, end, cap) = (trip[0], trip[1], trip[2]);
            let live = end.saturating_sub(begin) / 8;
            let total = cap.saturating_sub(begin) / 8;
            eprintln!(
                "home vec: begin=0x{begin:x} end=0x{end:x} cap=0x{cap:x} (live {live}, capacity {total})"
            );
            if heapish(begin) && cap > begin && (cap - begin) <= 0x200 {
                let arr = read_qwords(&game, begin, (cap - begin) as usize);
                for (i, &hp) in arr.iter().enumerate() {
                    let zone = if (i as u64) < live {
                        "LIVE [begin,end)"
                    } else {
                        "DEAD [end,cap)"
                    };
                    let nid = if heapish(hp) {
                        game.op_json("horse.read", &json!({ "addr": format!("0x{hp:x}") }))
                            .ok()
                            .and_then(|v| {
                                v.get("result")
                                    .unwrap_or(&v)
                                    .get("name_id")
                                    .and_then(Value::as_u64)
                            })
                    } else {
                        None
                    };
                    eprintln!("  [{i}] 0x{hp:x} name_id={nid:?}  {zone}");
                }
            }
        }
    }

    eprintln!("\n=== done ===");
}
