//! Research test: locate the TRUE function entries for the 4 stale
//! save addresses by scanning a wide window (+/-512 bytes) around
//! each Ghidra RVA and reporting every offset that passes the MSVC
//! entry-pattern classifier.
//!
//! Not a contract; it's a probe. Run, inspect the log, pick the
//! address whose prologue looks most distinctive, then derive a
//! signature from the next ~32 bytes for the real catalog.

mod common;

use modforge::harness::{GameHarness, RunningGame};
use serde_json::json;

const STALE: &[(&str, u64)] = &[
    ("SAVE_WRITER", 0x14006dc80),
    ("LOAD_GAME", 0x14006e480),
    ("HORSE_SAVE_WRITER", 0x14006ee10),
    ("HORSE_SAVE_LOADER", 0x14006f150),
];

const WINDOW_BEFORE: i64 = 2048;
const WINDOW_AFTER: i64 = 2048;

/// Tight classifier: only accepts the two distinctive MSVC entry
/// patterns that appear at the START of a function:
///
///   1. Shadow-space saves: `48 89 5c 24 ??` (mov [rsp+disp8], rbx)
///      OPTIONALLY preceded by another shadow-space save like
///      `48 89 74 24 ??` (mov [rsp+disp8], rsi). The vanilla
///      `FUN_1400aac60` and `FUN_1400a2d70` we already trust both
///      start with `48 89 5c 24 ??`.
///
///   2. Big-frame prologue: `48 8b c4` (mov rax, rsp) followed by
///      a push chain (`55 53 56 57 41 5x ...`). The vanilla
///      `FUN_14009f670` matches this exactly.
///
///   3. Plain push prologue: `57 41 5x` (push rdi + push r12-r15)
///      followed by `48 83 ec ??` (sub rsp, imm8) or `48 81 ec`
///      (sub rsp, imm32). HORSE_SAVE_WRITER's true entry matches
///      this.
use modforge::testkit::fn_entry::is_msvc_x64_prologue as looks_like_msvc_entry;

fn parse_hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .filter_map(|t| u8::from_str_radix(t, 16).ok())
        .collect()
}

fn read_bytes(game: &RunningGame, addr: u64, n: u32) -> Vec<u8> {
    let resp = game
        .op_json(
            "patterns.read_bytes",
            &json!({"addr": format!("0x{addr:x}"), "n": n}),
        )
        .expect("patterns.read_bytes must succeed");
    let hex = resp
        .get("result")
        .and_then(|r| r.get("bytes"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    parse_hex(&hex)
}

fn fetch_image_base(game: &RunningGame) -> u64 {
    let resp = game
        .op_json("game.build_info", &json!({}))
        .expect("game.build_info must succeed");
    let base = resp
        .get("result")
        .and_then(|r| r.get("image_base"))
        .and_then(|v| v.as_str())
        .unwrap_or("0x0");
    u64::from_str_radix(base.trim_start_matches("0x"), 16).unwrap_or(0)
}

#[test]
fn probe_save_entries() {
    if modforge::harness::should_skip() {
        eprintln!("MODFORGE_NO_GAME=1, skipping");
        return;
    }
    let spec = common::spec();
    let game = GameHarness::launch(&spec, "r2_save_find_entries").expect("harness launch failed");

    let image_base = fetch_image_base(&game);
    let preferred_base = 0x140000000u64;
    let delta = image_base.wrapping_sub(preferred_base);
    game.log().event(
        "PROBE",
        &format!("image_base=0x{image_base:x} delta=0x{delta:x}"),
    );

    for (name, stale_rva) in STALE {
        let stale_runtime = stale_rva.wrapping_add(delta);
        game.log().event(
            name,
            &format!("stale RVA=0x{stale_rva:x} runtime=0x{stale_runtime:x}"),
        );

        // Read a big window in <=4096-byte chunks (the read_bytes op
        // caps at 4096).
        let window_start = stale_runtime.wrapping_sub(WINDOW_BEFORE as u64);
        let total = (WINDOW_BEFORE + WINDOW_AFTER + 16) as usize;
        let mut bytes = Vec::with_capacity(total);
        let chunk = 2048usize;
        let mut off = 0usize;
        while off < total {
            let want = chunk.min(total - off) as u32;
            let part = read_bytes(&game, window_start + off as u64, want);
            if part.is_empty() {
                break;
            }
            bytes.extend(part);
            off += chunk;
        }
        if bytes.len() < 16 {
            game.log().event(
                name,
                &format!("short read ({} bytes, wanted {})", bytes.len(), total),
            );
            continue;
        }

        let mut candidates = Vec::new();
        for i in 0..(bytes.len().saturating_sub(16)) {
            if looks_like_msvc_entry(&bytes[i..i + 16]) {
                let abs = window_start.wrapping_add(i as u64);
                let rel: i64 = i as i64 - WINDOW_BEFORE;
                let prefix: String = bytes[i..i + 16]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                candidates.push((abs, rel, prefix));
            }
        }

        if candidates.is_empty() {
            game.log()
                .event(name, "no MSVC-entry candidates in +/-512 window");
        } else {
            game.log().event(
                name,
                &format!("{} candidates in +/-512 window:", candidates.len()),
            );
            // Print at most 8 nearest to stale to keep log readable.
            let mut sorted = candidates.clone();
            sorted.sort_by_key(|(_, rel, _)| rel.abs());
            for (abs, rel, prefix) in sorted.iter().take(8) {
                game.log().event(
                    name,
                    &format!("  offset={rel:+5}  abs=0x{abs:x}  [{prefix}]"),
                );
            }
        }
    }

    game.pass("probe completed; see log for entry candidates");
}
