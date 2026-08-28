//! Generic: probe-and-classify for a stale function-entry RVA.
//!
//! When a hardcoded function RVA from old decomp points
//! mid-function on the current build, this tool scans +/-WINDOW
//! bytes around it and reports every byte position whose bytes
//! match a known MSVC function-prologue pattern, sorted by
//! proximity to the stale RVA. The closest valid prologue is
//! usually the new entry.
//!
//! Env:
//!   MODFORGE_STALE_RVA  (required) PE-RVA where the old name pointed
//!   MODFORGE_WINDOW     (optional, default 2048) +/-N bytes to scan
//!
//! Example: find the true entry for a function whose old RVA was
//! 0x1400dde40:
//!   MODFORGE_STALE_RVA=0x1400dde40 \

mod common;

use serde_json::json;

fn need(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}
fn parse_hex_or_dec(s: &str) -> u64 {
    if let Some(stripped) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(stripped, 16).unwrap_or(0)
    } else {
        s.parse().unwrap_or(0)
    }
}

fn parse_hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .filter_map(|t| u8::from_str_radix(t, 16).ok())
        .collect()
}

fn read_bytes(game: &modforge::harness::RunningGame, addr: u64, n: u32) -> Vec<u8> {
    let resp = game
        .op_json(
            "patterns.read_bytes",
            &json!({"addr": format!("0x{addr:x}"), "n": n}),
        )
        .expect("read_bytes");
    let hex = resp
        .get("result")
        .and_then(|r| r.get("bytes"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    parse_hex(hex)
}

/// MSVC x64 function-entry classifier. Accepts the most common
/// prologue shapes; rejects mid-function bytes.
fn looks_like_entry(b: &[u8]) -> bool {
    if b.len() < 6 {
        return false;
    }
    // mov [rsp+disp8], rbx (shadow-space rbx save).
    if b[0] == 0x48 && b[1] == 0x89 && b[2] == 0x5c && b[3] == 0x24 {
        return true;
    }
    // mov rax, rsp + push chain.
    if b[0] == 0x48 && b[1] == 0x8b && b[2] == 0xc4 {
        return matches!(b[3], 0x55 | 0x53 | 0x56 | 0x57 | 0x40..=0x4f);
    }
    // push rdi + push r12-r15 + sub rsp.
    if b[0] == 0x57 && b[1] == 0x41 && matches!(b[2], 0x54..=0x57) {
        return matches!(b[3], 0x41 | 0x48 | 0x55 | 0x56 | 0x57);
    }
    // mov [rsp+disp8], rsi/rdi/rbp (other shadow-space saves).
    if b[0] == 0x48 && b[1] == 0x89 && matches!(b[2], 0x6c | 0x74 | 0x7c) && b[3] == 0x24 {
        return true;
    }
    // push rbx then push rsi/rdi.
    if b[0] == 0x53 && matches!(b[1], 0x55 | 0x56 | 0x57) {
        return true;
    }
    // push rsi + push rdi + sub rsp.
    if b[0] == 0x56 && b[1] == 0x57 && b[2] == 0x48 {
        return matches!(b[3], 0x83 | 0x81);
    }
    false
}

#[test]
fn probe_function_entries_around_stale_rva() {
    let Some(stale_str) = need("MODFORGE_STALE_RVA") else {
        eprintln!("skipping: set MODFORGE_STALE_RVA to use");
        return;
    };
    let stale_rva = parse_hex_or_dec(&stale_str);
    let window = need("MODFORGE_WINDOW")
        .map(|s| parse_hex_or_dec(&s) as i64)
        .unwrap_or(2048);

    let Some(game) = common::launch("research_find_function_entry") else {
        return;
    };
    let r = game
        .op_json("targets.resolve.gamestate_ptr", &json!({}))
        .expect("ok");
    let ib = u64::from_str_radix(
        r.get("result")
            .unwrap()
            .get("image_base")
            .unwrap()
            .as_str()
            .unwrap()
            .trim_start_matches("0x"),
        16,
    )
    .unwrap();
    let runtime = ib + (stale_rva - 0x140000000);

    let window_start = runtime.wrapping_sub(window as u64);
    let bytes = read_bytes(&game, window_start, (window as u32) * 2);
    let mut hits: Vec<(i64, usize)> = Vec::new();
    for off in 0..bytes.len().saturating_sub(6) {
        if looks_like_entry(&bytes[off..]) {
            let addr = window_start + off as u64;
            let dist = (addr as i64) - (runtime as i64);
            hits.push((dist.abs(), off));
        }
    }
    hits.sort();
    game.log().event(
        "PROBE",
        &format!(
            "stale_rva=0x{stale_rva:x} runtime=0x{runtime:x} window={window} candidates={}",
            hits.len()
        ),
    );
    for (i, (_, off)) in hits.iter().take(10).enumerate() {
        let addr = window_start + *off as u64;
        let dist = (addr as i64) - (runtime as i64);
        let preview: String = bytes[*off..(*off + 32).min(bytes.len())]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let rva = stale_rva.wrapping_add_signed(dist);
        game.log().event(
            "CANDIDATE",
            &format!("[{i}] addr=0x{addr:x} rva=0x{rva:x} dist={dist:+} bytes={preview}"),
        );
    }
    game.pass(&format!(
        "probed stale RVA 0x{stale_rva:x}; {} candidates within +/-{window}",
        hits.len()
    ));
}
