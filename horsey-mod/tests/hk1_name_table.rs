//! Re-derive NAME_TABLE for the current build. The name resolver
//! FUN_1400c78c0 computes `entry = *NAME_TABLE_slot + name_id * 0x88`,
//! then reads an MSVC std::string at that entry. The hardcoded slot RVA
//! is stale, and the custom scanner isn't finding the table (names read
//! "?"). The 0x88 stride imul is the anchor: 0x88 > 127 forces an imm32,
//! so MSVC emits `69 ?? 88 00 00 00` (the prior attempt scanned for an
//! imm8 form that can't exist). Near the imul is the rip-relative load
//! of the NAME_TABLE slot. This dumps the current resolver state plus
//! every 0x88 site with surrounding bytes so the real slot can be read.

mod common;

use serde_json::{Value, json};

fn hex_at(game: &modforge::harness::RunningGame, abs: u64, n: usize) -> String {
    let v = game
        .op_json(
            "patterns.read_bytes",
            &json!({"addr": format!("0x{abs:x}"), "n": n}),
        )
        .unwrap_or(Value::Null);
    v.get("result")
        .unwrap_or(&v)
        .get("bytes")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

#[test]
fn rederive_name_table() {
    let Some(game) = common::launch("hk1_name_table") else {
        return;
    };

    // Wait for load + collect horse name_ids.
    let dl = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut horses: Vec<Value> = vec![];
    loop {
        let v = game
            .op_json("gamestate.owned_horses", &json!({}))
            .unwrap_or(Value::Null);
        let r = v.get("result").unwrap_or(&v);
        horses = r
            .get("horses")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !horses.is_empty() || std::time::Instant::now() >= dl {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    eprintln!("[HORSES] {} owned:", horses.len());
    for h in &horses {
        let idx = h.get("idx").and_then(Value::as_u64).unwrap_or(0);
        let name = h.get("name").and_then(Value::as_str).unwrap_or("<none>");
        let nid = h.get("name_id").and_then(Value::as_u64).unwrap_or(0);
        let c = h.get("container").and_then(Value::as_str).unwrap_or("?");
        let sx = h.get("scene_x").and_then(Value::as_f64).unwrap_or(0.0);
        let sy = h.get("scene_y").and_then(Value::as_f64).unwrap_or(0.0);
        eprintln!(
            "  horse[{idx}] name={name:?} name_id={nid} container={c} scene=({sx:.2}, {sy:.2})"
        );
    }
    let nids: Vec<u64> = horses
        .iter()
        .filter_map(|h| h.get("name_id").and_then(Value::as_u64))
        .collect();
    let nid0 = nids.first().copied().unwrap_or(0);

    // Current resolver state for one name_id.
    if let Ok(d) = game.op_json("horse.name_diag", &json!({"name_id": nid0})) {
        eprintln!(
            "[NAME_DIAG nid={nid0}] {}",
            serde_json::to_string_pretty(d.get("result").unwrap_or(&d)).unwrap()
        );
    }

    // Scan for the *0x88 stride imul (catches both the bare and REX-prefixed forms).
    let scan = game
        .op_json(
            "patterns.sleuth.scan_all",
            &json!({"sig": "69 ?? 88 00 00 00", "disp32_offset": 2, "instr_len": 6, "max_hits": 32}),
        )
        .expect("scan_all");
    let hits = scan
        .get("result")
        .unwrap_or(&scan)
        .get("hits")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    eprintln!("[SCAN] {} site(s) with imul *0x88:", hits.len());
    for h in &hits {
        let instr = h.get("instr_addr").and_then(Value::as_str).unwrap_or("0x0");
        let abs = u64::from_str_radix(instr.trim_start_matches("0x"), 16).unwrap_or(0);
        // Dump from one byte before (REX) through 21 bytes after the imul,
        // so the rip-relative slot load (add/mov r64,[rip+disp32]) is visible.
        let bytes = hex_at(&game, abs.saturating_sub(1), 28);
        eprintln!("  @ {instr}: {bytes}");
    }

    // Replicate the resolver: targeted scan -> decode slot -> deref -> entry.
    let t = game
        .op_json(
            "patterns.sleuth.scan_all",
            &json!({"sig": "48 69 c0 88 00 00 00 48 03 05 ?? ?? ?? ??", "disp32_offset": 10, "instr_len": 14, "max_hits": 8}),
        )
        .expect("targeted scan");
    let th = t
        .get("result")
        .unwrap_or(&t)
        .get("hits")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    eprintln!("[TARGETED] sig matched {} site(s)", th.len());
    let _ = nid0;
    let bytes_at = |abs: u64, n: usize| -> Vec<u8> {
        hex_at(&game, abs, n)
            .split_whitespace()
            .filter_map(|x| u8::from_str_radix(x, 16).ok())
            .collect()
    };
    if let Some(h0) = th.first() {
        let slot = h0
            .get("decoded_target")
            .and_then(Value::as_str)
            .unwrap_or("0x0");
        let sv = u64::from_str_radix(slot.trim_start_matches("0x"), 16).unwrap_or(0);
        let tb = bytes_at(sv, 8);
        if tb.len() >= 8 {
            let table = u64::from_le_bytes(tb[..8].try_into().unwrap());
            eprintln!("[NAMES] table base = 0x{table:x}");
            for h in &horses {
                let idx = h.get("idx").and_then(Value::as_u64).unwrap_or(0);
                let nid = h.get("name_id").and_then(Value::as_u64).unwrap_or(0);
                let c = h.get("container").and_then(Value::as_str).unwrap_or("?");
                let entry = table + nid * 0x88;
                let head = bytes_at(entry, 0x20);
                let name = if head.len() >= 0x20 {
                    let size = u64::from_le_bytes(head[0x10..0x18].try_into().unwrap()) as usize;
                    let cap = u64::from_le_bytes(head[0x18..0x20].try_into().unwrap()) as usize;
                    if size == 0 || size > 64 {
                        format!("<size {size}>")
                    } else if cap > 0xF {
                        let p = u64::from_le_bytes(head[0..8].try_into().unwrap());
                        String::from_utf8_lossy(&bytes_at(p, size)).into_owned()
                    } else {
                        String::from_utf8_lossy(&head[0..size]).into_owned()
                    }
                } else {
                    "<unreadable>".into()
                };
                eprintln!("  horse[{idx}] NAME=\"{name}\"  container={c}  name_id={nid}");
            }
        }
    }
}
