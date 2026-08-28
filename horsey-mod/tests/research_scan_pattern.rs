//! Generic: scan `.text` for one IDA-style signature, decode each
//! match's disp32 to a CPU-effective target address, dump hits.
//!
//! Drives discovery of new data globals (cheat flags, counter
//! globals, save-format addresses, etc.) by anchoring on a UNIQUE
//! byte sequence that references the global.
//!
//! Env:
//!   MODFORGE_SIG          (required) IDA-style sig with `??` wildcards
//!   MODFORGE_DISP32_OFF   (required) byte offset of the disp32 inside
//!                                    the match (typically 2 for
//!                                    `c6 05 dd dd dd dd 01` etc.)
//!   MODFORGE_INSTR_LEN    (required) full instruction length so CPU's
//!                                    next_ip = match_start + INSTR_LEN
//!   MODFORGE_MAX_HITS     (optional, default 256)
//!   MODFORGE_CONTEXT_BYTES (optional, default 16)
//!
//! Example: scan for `mov byte [rip+disp32], 1` (sets a byte global):
//!   MODFORGE_SIG="c6 05 ?? ?? ?? ?? 01" \
//!   MODFORGE_DISP32_OFF=2 MODFORGE_INSTR_LEN=7 \

mod common;

use serde_json::{Value, json};

fn need(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

fn parse_u32(s: &str) -> u32 {
    s.parse()
        .unwrap_or_else(|_| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0))
}

#[test]
fn scan_and_decode_pattern() {
    let Some(sig) = need("MODFORGE_SIG") else {
        eprintln!("skipping: set MODFORGE_SIG + MODFORGE_DISP32_OFF + MODFORGE_INSTR_LEN to use");
        return;
    };
    let disp_off = need("MODFORGE_DISP32_OFF")
        .map(|s| parse_u32(&s))
        .expect("MODFORGE_DISP32_OFF required");
    let instr_len = need("MODFORGE_INSTR_LEN")
        .map(|s| parse_u32(&s))
        .expect("MODFORGE_INSTR_LEN required");
    let max_hits = need("MODFORGE_MAX_HITS")
        .map(|s| parse_u32(&s))
        .unwrap_or(256);
    let context = need("MODFORGE_CONTEXT_BYTES")
        .map(|s| parse_u32(&s))
        .unwrap_or(16);

    let Some(game) = common::launch("forensic_scan_pattern") else {
        return;
    };
    let resp = game
        .op_json(
            "patterns.sleuth.scan_all",
            &json!({
                "sig": sig,
                "disp32_offset": disp_off,
                "instr_len": instr_len,
                "context_bytes": context,
                "max_hits": max_hits,
            }),
        )
        .expect("patterns.sleuth.scan_all must succeed");
    let result = resp.get("result").expect("result");
    let hits: Vec<Value> = result
        .get("hits")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    game.log().event(
        "SCAN",
        &format!(
            "sig={sig:?} disp_off={disp_off} instr_len={instr_len} hits={}",
            hits.len()
        ),
    );
    for h in &hits {
        game.log().event(
            "HIT",
            &format!(
                "instr@{} target={} disp32={} ctx={}",
                h.get("instr_addr").and_then(|v| v.as_str()).unwrap_or("?"),
                h.get("decoded_target")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?"),
                h.get("disp32").and_then(|v| v.as_str()).unwrap_or("?"),
                h.get("context_hex").and_then(|v| v.as_str()).unwrap_or("?"),
            ),
        );
    }
    game.pass(&format!("scanned sig={sig:?}, {} hits logged", hits.len()));
}
