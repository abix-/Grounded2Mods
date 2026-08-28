//! HK1 research: snapshot a horse's full 0x498 bytes, wait for the
//! operator to perform a real in-game container change (drag pasture
//! <-> truck), then snapshot again and report every offset that
//! changed. The differing offsets isolate the CAUSAL container fields,
//! ruling out the downstream-only +0x1d0 display we already proved
//! doesn't move the horse when poked directly.
//!
//! Env vars:
//! - `HORSEY_HORSE=<name>`              target horse (default first owned)
//! - `HK1_SNAPSHOT_WAIT_SECS=<n>`       seconds between snapshots (default 30)
//! - `HK1_DIFF_IGNORE_TICKER=1`         skip the +0x008 frame counter (default on)
//!
//! Designed for MODFORGE_ATTACH=1 against an already-running session.

mod common;

use serde_json::{Value, json};
use std::time::{Duration, Instant};

const HORSE_SIZE: usize = 0x498;

#[test]
fn snapshot_diff_horse() {
    let Some(game) = common::launch("hk1_snapshot_diff_horse") else {
        return;
    };
    let h = common::wait_for_target_horse(&game, Duration::from_secs(60));
    eprintln!(
        "\n[target] horse '{}' at {} (raw {:#x})",
        h.name, h.ptr_s, h.id
    );

    let wait_secs: u64 = common::env_or("HK1_SNAPSHOT_WAIT_SECS", 30u64);
    let ignore_ticker: bool = std::env::var("HK1_DIFF_IGNORE_TICKER").ok().as_deref() != Some("0");

    let before = read_horse_bytes(&game, &h.ptr_s);

    eprintln!("\n=== SNAPSHOT A captured ({} bytes) ===", before.len());
    eprintln!(
        ">>> NOW: drag '{}' between containers in-game (you have {wait_secs}s) <<<",
        h.name
    );
    let t0 = Instant::now();
    while t0.elapsed().as_secs() < wait_secs {
        std::thread::sleep(Duration::from_secs(1));
        let remaining = wait_secs.saturating_sub(t0.elapsed().as_secs());
        if remaining % 5 == 0 && remaining > 0 {
            eprintln!("  ... {remaining}s remaining");
        }
    }

    let after = read_horse_bytes(&game, &h.ptr_s);
    eprintln!("\n=== SNAPSHOT B captured ===");

    assert_eq!(before.len(), HORSE_SIZE, "snapshot A wrong size");
    assert_eq!(after.len(), HORSE_SIZE, "snapshot B wrong size");

    let mut diffs: Vec<(usize, u8, u8)> = Vec::new();
    for i in 0..HORSE_SIZE {
        if before[i] != after[i] {
            diffs.push((i, before[i], after[i]));
        }
    }

    let ticker_range = 0x008..0x00c;
    let interesting: Vec<_> = diffs
        .iter()
        .filter(|(o, _, _)| !(ignore_ticker && ticker_range.contains(o)))
        .collect();

    eprintln!("\nraw differences: {} bytes", diffs.len());
    eprintln!("interesting (ticker excluded): {} bytes", interesting.len());
    eprintln!("\n  off    before  after  ascii_b  ascii_a");
    for (o, b, a) in &interesting {
        let pb = if b.is_ascii_graphic() {
            *b as char
        } else {
            '.'
        };
        let pa = if a.is_ascii_graphic() {
            *a as char
        } else {
            '.'
        };
        eprintln!("  +0x{o:03x}  0x{b:02x}    0x{a:02x}    {pb:>4}     {pa:>4}");
    }

    // Group runs of consecutive changed bytes into u32 / u64 fields
    // so we can see "this whole qword changed" patterns.
    eprintln!("\nclustered runs:");
    let mut i = 0;
    while i < interesting.len() {
        let start = interesting[i].0;
        let mut end = start;
        while i + 1 < interesting.len() && interesting[i + 1].0 == end + 1 {
            end = interesting[i + 1].0;
            i += 1;
        }
        let len = end - start + 1;
        let mut before_word = 0u64;
        let mut after_word = 0u64;
        for k in 0..len.min(8) {
            before_word |= (before[start + k] as u64) << (k * 8);
            after_word |= (after[start + k] as u64) << (k * 8);
        }
        eprintln!(
            "  +0x{start:03x}..+0x{end:03x}  ({len} byte{}): 0x{before_word:x} -> 0x{after_word:x}",
            if len == 1 { "" } else { "s" }
        );
        i += 1;
    }

    game.pass(&format!(
        "horse='{}' diffs={} interesting={}",
        h.name,
        diffs.len(),
        interesting.len()
    ));
}

fn read_horse_bytes(game: &modforge::harness::RunningGame, ptr_s: &str) -> Vec<u8> {
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
    hex.split_whitespace()
        .map(|s| u8::from_str_radix(s, 16).unwrap_or(0))
        .collect()
}
