//! HK1 Stage S0.5: dump every owned horse's bytes and print a
//! per-offset variance map. Bytes that vary across the 3 owned horses
//! AND are small ints / small enums are container-state candidates.
//! Bytes that are identical across all owned horses are class-level
//! constants (vtable, alloc fields) and can be ignored.
//!
//! Run with MODFORGE_ATTACH=1 against a running game so the user's
//! placement (tomtato in truck, the other 2 wherever) is preserved.

mod common;

use serde_json::{Value, json};

const HORSE_SIZE: usize = 0x498;

#[test]
fn diff_all_owned_horses() {
    let Some(game) = common::launch("hk1_diff_all_owned_horses") else {
        return;
    };
    let horses = common::list_owned(&game);
    assert!(!horses.is_empty(), "no owned horses");
    eprintln!("\nowned horses: {}", horses.len());

    let mut dumps: Vec<(String, String, Vec<u8>)> = Vec::new();
    for h in &horses {
        let name = h
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string();
        let ptr_s = h
            .get("ptr")
            .and_then(Value::as_str)
            .unwrap_or("0x0")
            .to_string();
        let resp = game
            .op_json(
                "patterns.read_bytes",
                &json!({
                    "addr": ptr_s, "n": HORSE_SIZE as u64,
                }),
            )
            .expect("read_bytes");
        let hex = resp
            .get("result")
            .and_then(|r| r.get("bytes"))
            .and_then(Value::as_str)
            .expect("no bytes")
            .to_string();
        let bytes: Vec<u8> = hex
            .split_whitespace()
            .map(|s| u8::from_str_radix(s, 16).unwrap_or(0))
            .collect();
        assert_eq!(bytes.len(), HORSE_SIZE, "wrong byte count for {name}");
        eprintln!("  {name:16} {ptr_s}");
        dumps.push((name, ptr_s, bytes));
    }

    // Per-offset variance: how many distinct byte values across horses?
    // 1 = identical, 2 = some differ, == count = all differ.
    let mut variant_offsets: Vec<(usize, Vec<u8>)> = Vec::new();
    for off in 0..HORSE_SIZE {
        let mut seen = Vec::with_capacity(dumps.len());
        for (_, _, b) in &dumps {
            seen.push(b[off]);
        }
        let mut uniq = seen.clone();
        uniq.sort_unstable();
        uniq.dedup();
        if uniq.len() > 1 {
            variant_offsets.push((off, seen));
        }
    }

    // Skip the 480-byte diploid genome (0x2b8..0x498). User maxed
    // tomtato; that whole region will differ for sure but isn't
    // container state. Also skip the well-known per-horse identity
    // fields: name_id (+0x1f8), age (+0x1fc), max_age (+0x200),
    // skill (+0x21c), litter (+0x254), and the heap pointers at
    // +0x040..+0x0c0 (sub-objects, allocator-driven).
    fn is_known_per_horse(off: usize) -> bool {
        (0x2b8..0x498).contains(&off)         // diploid genome
            || (0x040..0x0c0).contains(&off)  // sub-object pointers
            || (0x1f8..0x1fc).contains(&off)  // name_id u32
            || (0x1fc..0x200).contains(&off)  // age u32
            || (0x200..0x204).contains(&off)  // max_age u32
            || (0x21c..0x220).contains(&off)  // skill u32
            || (0x254..0x258).contains(&off)  // litter u32
            || (0x180..0x1a0).contains(&off)  // type label "SmartHuman" etc
            || (0x000..0x008).contains(&off) // vtable
    }

    let interesting: Vec<_> = variant_offsets
        .iter()
        .filter(|(o, _)| !is_known_per_horse(*o))
        .collect();

    eprintln!("\nvariant offsets (excluding known per-horse fields):");
    eprintln!("  total interesting offsets: {}", interesting.len());
    eprintln!(
        "\n  off    {}",
        dumps
            .iter()
            .map(|(n, _, _)| format!("{n:>10.10}"))
            .collect::<Vec<_>>()
            .join("  ")
    );
    for (off, vals) in &interesting {
        let row: String = vals
            .iter()
            .map(|b| format!("       0x{b:02x}"))
            .collect::<Vec<_>>()
            .join("  ");
        eprintln!("  +0x{off:03x} {row}");
    }
    game.pass(&format!(
        "variance scan: {} interesting per-byte differences",
        interesting.len()
    ));
}
