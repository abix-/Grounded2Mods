//! Investigate why the Horses tab shows wrong counts.
//!
//! Context: user owns 3 horses in pasture. UI Horses tab shows
//! `live=0, total=<drifts 1..4>`. Decomp says:
//!   * Roster (GameState +0x280/+0x288, stride 0x24) is the right
//!     "horse list" but entries are 36-byte SUMMARIES, not full Horse
//!     objects.
//!   * Live-horse pointer vector is on a nested struct reached via
//!     `*(GameState + 0x438) -> + ?? -> +0x130/+0x138`, not at
//!     `GameState + 0x130` directly.
//!
//! This test (attach mode):
//!   1. Reads GameState ptr + roster begin/end via gamestate.diag.
//!   2. Walks each 36-byte roster entry. Dumps 4 qwords + 4 bytes.
//!   3. For each qword inside an entry, checks heap-shape and (if
//!      plausible) dereferences and reads candidate Horse fields:
//!      age (i32 at +0x1fc), tired_a (u8 at +0x205). Plausible age
//!      in [0, 200] + tired in {0,1} = strong Horse* signal.
//!   4. Reads 8 bytes at GS+0x438 (sub-struct ptr), then 0x300 bytes
//!      of that sub-struct, scans for begin/end pointer pairs with
//!      a small element count.
//!
//! Modes (env-driven):
//! - default: discovery. Prints findings; assertions are permissive
//!   (chain returns plausible structure).
//! - `HORSEY_EXPECT_OWNED=<N>`: strict. Assert at least one slot in
//!   the GS+0x438 sub-struct has a horse vector of length N.
//!
//! Run with the game already running + a save loaded:
//!   $env:MODFORGE_ATTACH = "1"
//!   $env:HORSEY_EXPECT_OWNED = "3"
//!   k3sc cargo-lock test -p horsey-mod --test investigate_horse_count. --nocapture

mod common;

use serde_json::{Value, json};

const ROSTER_STRIDE: usize = 0x24;
const SUB_STRUCT_OFF: usize = 0x438;
const HORSE_AGE_OFF: usize = 0x1fc;
const HORSE_TIRED_A_OFF: usize = 0x205;

fn parse_hex(s: &str) -> usize {
    usize::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)
}

fn try_read_bytes(game: &modforge::harness::RunningGame, addr: usize, n: usize) -> Option<Vec<u8>> {
    let resp = game
        .op_json(
            "patterns.read_bytes",
            &json!({ "addr": format!("0x{addr:x}"), "n": n }),
        )
        .ok()?;
    let hex = resp
        .get("result")
        .and_then(|r| r.get("bytes"))
        .and_then(Value::as_str)?;
    Some(
        hex.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect(),
    )
}

fn read_bytes_must(game: &modforge::harness::RunningGame, addr: usize, n: usize) -> Vec<u8> {
    try_read_bytes(game, addr, n)
        .unwrap_or_else(|| panic!("patterns.read_bytes failed at 0x{addr:x} n={n}"))
}

fn read_u64(bytes: &[u8], off: usize) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[off..off + 8]);
    u64::from_le_bytes(buf)
}

fn read_u32(bytes: &[u8], off: usize) -> u32 {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&bytes[off..off + 4]);
    u32::from_le_bytes(buf)
}

fn read_i32(bytes: &[u8], off: usize) -> i32 {
    read_u32(bytes, off) as i32
}

fn heap_shaped(addr: u64) -> bool {
    // Win10 x64 heap typically lives in [0x0000_0001_0000_0000, 0x0000_7FFF_FFFF_FFFF).
    // 0x20000001c-style values pass a loose 0x10000 check but aren't real heap
    // addresses and will fault when dereferenced. Require the upper 32 bits to
    // be non-zero to filter that out.
    addr >= 0x1_0000_0000 && addr < 0x7fff_ffff_ffff && (addr & 0x7) == 0
}

#[test]
fn investigate_roster_and_live_chain() {
    let Some(game) = common::launch("investigate_horse_count") else {
        return;
    };
    let expected_owned: Option<usize> = std::env::var("HORSEY_EXPECT_OWNED")
        .ok()
        .map(|s| s.parse().expect("HORSEY_EXPECT_OWNED not a number"));
    let exp = expected_owned.unwrap_or(0);

    // --- Step 1: gamestate.diag for ptr + roster span ---
    let diag = game
        .op_json("gamestate.diag", &json!({}))
        .expect("gamestate.diag should succeed");
    let result = diag.get("result").unwrap_or(&diag);

    let gs_ptr_s = result
        .get("ptr")
        .and_then(Value::as_str)
        .expect("diag missing ptr");
    let gs_ptr = parse_hex(gs_ptr_s);
    assert!(
        heap_shaped(gs_ptr as u64),
        "GameState ptr not heap-shaped: {gs_ptr_s}"
    );
    eprintln!("\n=== GameState @ 0x{gs_ptr:x} ===");

    let roster = result
        .get("roster_0x280_0x288")
        .expect("diag missing roster");
    let r_begin = parse_hex(roster.get("begin").and_then(Value::as_str).unwrap());
    let r_end = parse_hex(roster.get("end").and_then(Value::as_str).unwrap());
    let span = r_end.saturating_sub(r_begin);
    let count = span / ROSTER_STRIDE;
    eprintln!(
        "Roster: begin=0x{r_begin:x} end=0x{r_end:x} span={span} count={count} (stride {ROSTER_STRIDE})"
    );

    // --- Step 2: dump each 36-byte roster entry ---
    assert!(
        count > 0,
        "roster is empty; load a save before running this test"
    );
    let entry_bytes = read_bytes_must(&game, r_begin, count * ROSTER_STRIDE);
    assert_eq!(entry_bytes.len(), count * ROSTER_STRIDE);

    let mut horse_ptr_slots: Vec<usize> = Vec::new();
    for i in 0..count {
        let base = i * ROSTER_STRIDE;
        let e = &entry_bytes[base..base + ROSTER_STRIDE];
        let q0 = read_u64(e, 0);
        let q1 = read_u64(e, 8);
        let q2 = read_u64(e, 16);
        let q3 = read_u64(e, 24);
        let tail4 = read_u32(e, 32);
        eprintln!(
            "Entry[{i}]@0x{:x}: q0=0x{q0:016x} q1=0x{q1:016x} q2=0x{q2:016x} q3=0x{q3:016x} tail32=0x{tail4:08x}",
            r_begin + base
        );

        // For each qword that looks heap-shaped, deref and probe.
        for (slot, qv) in [(0usize, q0), (8, q1), (16, q2), (24, q3)] {
            if !heap_shaped(qv) {
                continue;
            }
            let Some(probe) = try_read_bytes(&game, qv as usize, 0x220) else {
                eprintln!("  slot+{slot:02}: deref 0x{qv:x} -> read failed (bad address)");
                continue;
            };
            if probe.len() < HORSE_TIRED_A_OFF + 1 {
                continue;
            }
            let age = read_i32(&probe, HORSE_AGE_OFF);
            let tired_a = probe[HORSE_TIRED_A_OFF];
            let plausible_age = (0..=200).contains(&age);
            let plausible_tired = tired_a <= 1;
            eprintln!(
                "  slot+{slot:02}: deref 0x{qv:x} -> age@0x1fc={age} tired_a@0x205={tired_a} \
                 [age_ok={plausible_age} tired_ok={plausible_tired}]"
            );
            if plausible_age && plausible_tired {
                if !horse_ptr_slots.contains(&slot) {
                    horse_ptr_slots.push(slot);
                }
            }
        }
    }

    // --- Step 3: GS+0x438 sub-struct chain ---
    eprintln!("\n=== GS+0x{SUB_STRUCT_OFF:x} sub-struct chain ===");
    let gs_block = read_bytes_must(&game, gs_ptr + SUB_STRUCT_OFF, 8);
    let sub_ptr = read_u64(&gs_block, 0);
    eprintln!("GS+0x{SUB_STRUCT_OFF:x} = 0x{sub_ptr:016x}");
    if heap_shaped(sub_ptr) {
        let Some(sub) = try_read_bytes(&game, sub_ptr as usize, 0x300) else {
            eprintln!("  sub-struct read failed");
            return;
        };
        // Scan for begin/end pointer pairs: adjacent qwords where both
        // are heap-shaped and (end - begin) / 8 in [1, 200].
        let mut found_pairs: Vec<(usize, u64, u64, usize)> = Vec::new();
        let mut matched_expected = false;
        let mut off = 0usize;
        while off + 16 <= sub.len() {
            let b = read_u64(&sub, off);
            let e = read_u64(&sub, off + 8);
            if heap_shaped(b) && heap_shaped(e) && e >= b {
                let span = (e - b) as usize;
                if span > 0 && span % 8 == 0 {
                    let n = span / 8;
                    if (1..=200).contains(&n) {
                        found_pairs.push((off, b, e, n));
                    }
                }
            }
            off += 8;
        }
        for (off, b, e, n) in &found_pairs {
            eprintln!("  pair@+0x{off:03x}: begin=0x{b:x} end=0x{e:x} count={n}",);
            if expected_owned == Some(*n) {
                eprintln!(
                    "  ^^^ MATCH: count == HORSEY_EXPECT_OWNED ({n}). Likely live-horse vector at *(GS+0x438)+0x{off:x}"
                );
                matched_expected = true;
            }
        }
        if found_pairs.is_empty() {
            eprintln!("  no pointer pairs found in first 0x300 bytes of sub-struct");
        }
        if let Some(want) = expected_owned {
            assert!(
                matched_expected,
                "HORSEY_EXPECT_OWNED={want}: no pointer pair in *(GS+0x438) had that count. \
                 found_pairs={found_pairs:?}"
            );
        }
    } else {
        eprintln!("  sub-struct pointer not heap-shaped; chain dead at this offset");
        if expected_owned.is_some() {
            panic!("HORSEY_EXPECT_OWNED set but GS+0x438 chain is dead");
        }
    }

    // --- Step 4: report findings ---
    eprintln!("\n=== SUMMARY ===");
    eprintln!("expected_owned_horses: {exp}");
    eprintln!("roster_count (GS+0x280/+0x288 / 0x24): {count}");
    eprintln!(
        "roster_entries_contain_horse_ptr_at_byte_offsets: {:?}",
        horse_ptr_slots
    );
    if count != exp {
        eprintln!(
            "NOTE: roster_count ({count}) != owned ({exp}). The roster is not the \
             owned-horse list; it includes more (NPCs / ancestors / pool)."
        );
    }
    if horse_ptr_slots.is_empty() {
        eprintln!(
            "NOTE: no roster-entry slot dereferenced to a plausible Horse*. Entries are likely \
             inline 36-byte horse summaries, not pointer wrappers."
        );
    }

    game.pass("investigation complete; see stderr for offsets");
}
