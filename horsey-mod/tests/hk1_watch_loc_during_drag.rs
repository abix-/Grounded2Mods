//! HK1 research: poll the Home Location's first 0x240 bytes at high
//! rate during a real user-driven horse drag. Report every (offset,
//! before, after) transition during the window.
//!
//! Reveals the EXACT set of LOC fields the game's own input handler
//! mutates when a drag happens, so we can stage them properly before
//! calling vtable[+0x78] without crashing.
//!
//! Designed for MODFORGE_ATTACH=1. User: load save, run this test,
//! perform ONE pasture <-> truck drag while it's polling. Test reports
//! the full transition trace.

mod common;

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const LOC_DUMP_LEN: usize = 0x240;

#[test]
fn watch_loc_during_drag() {
    let Some(game) = common::launch("hk1_watch_loc_during_drag") else {
        return;
    };
    let _ = common::wait_for_target_horse(&game, Duration::from_secs(30));

    let secs: u64 = common::env_or("HK1_WATCH_SECS", 30u64);
    let hz: u64 = common::env_or("HK1_WATCH_HZ", 20u64);
    let interval = Duration::from_millis(1000 / hz);

    eprintln!("watching LOC[0..{LOC_DUMP_LEN:#x}] at {hz} Hz for {secs}s");
    eprintln!(">>> DO YOUR DRAG NOW (pasture <-> truck) <<<");

    let baseline = dump_loc(&game);
    let mut prev = baseline.clone();
    let mut all_transitions: BTreeMap<usize, Vec<(u64, u8, u8)>> = BTreeMap::new();

    let t0 = Instant::now();
    let deadline = t0 + Duration::from_secs(secs);
    while Instant::now() < deadline {
        std::thread::sleep(interval);
        let now = dump_loc(&game);
        let ms = t0.elapsed().as_millis() as u64;
        for (i, (&b, &a)) in prev.iter().zip(now.iter()).enumerate() {
            if b != a {
                all_transitions.entry(i).or_default().push((ms, b, a));
            }
        }
        prev = now;
    }

    let final_state = prev;

    eprintln!(
        "\n=== summary: {} offsets changed during {secs}s window ===",
        all_transitions.len()
    );

    // Group consecutive offsets into runs for readability.
    let offsets: Vec<usize> = all_transitions.keys().copied().collect();
    let mut runs: Vec<(usize, usize)> = Vec::new(); // (start, end inclusive)
    let mut i = 0;
    while i < offsets.len() {
        let start = offsets[i];
        let mut end = start;
        while i + 1 < offsets.len() && offsets[i + 1] == end + 1 {
            end = offsets[i + 1];
            i += 1;
        }
        runs.push((start, end));
        i += 1;
    }

    eprintln!("\nclustered runs ({}):", runs.len());
    for (s, e) in &runs {
        let len = e - s + 1;
        let mut b_word = 0u64;
        let mut a_word = 0u64;
        for k in 0..len.min(8) {
            b_word |= (baseline[s + k] as u64) << (k * 8);
            a_word |= (final_state[s + k] as u64) << (k * 8);
        }
        eprintln!(
            "  +0x{s:03x}..+0x{e:03x}  ({len:>3} bytes)  baseline=0x{b_word:x}  final=0x{a_word:x}  transitions={}",
            len
        );
    }

    eprintln!("\nper-offset transition trace (interesting offsets only, max 20 transitions each):");
    for (off, trans) in &all_transitions {
        if trans.len() <= 1 {
            continue;
        }
        let n = trans.len().min(20);
        let trace: String = trans
            .iter()
            .take(n)
            .map(|(t, b, a)| format!("@{t}ms 0x{b:02x}->0x{a:02x}"))
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!("  +0x{off:03x}  ({} hits): {trace}", trans.len());
    }

    game.pass(&format!(
        "captured {} changed offsets in {secs}s",
        all_transitions.len()
    ));
    std::process::exit(0);
}

fn dump_loc(game: &modforge::harness::RunningGame) -> Vec<u8> {
    let r = game
        .op_json(
            "hk1.loc_bytes",
            &json!({ "offset": 0, "n": LOC_DUMP_LEN as u64 }),
        )
        .expect("hk1.loc_bytes");
    let hex = r
        .get("result")
        .and_then(|x| x.get("bytes"))
        .and_then(Value::as_str)
        .expect("no bytes")
        .to_string();
    hex.split_whitespace()
        .map(|s| u8::from_str_radix(s, 16).unwrap_or(0))
        .collect()
}
