//! Generic: scan a sig that has TWO disp32s, decode both per match,
//! and optionally filter by the inter-target delta.
//!
//! Use when the discriminator for a target is the RELATIONSHIP
//! between two RIP-relative references inside a single contiguous
//! instruction sequence. Example: the d-e-b-u-g unlock block is
//! `c6 05 dd1 01 c6 05 dd2 00` where target2 - target1 == -0x79 by
//! virtue of the decomp's `.data` layout; this delta uniquely
//! identifies DEBUG_MODE_ACTIVE.
//!
//! Env:
//!   MODFORGE_SIG          (required) IDA-style sig
//!   MODFORGE_DISP1_OFF    (required) byte offset of first disp32
//!   MODFORGE_DISP1_NEXT_IP (required) match-relative offset of
//!                                     CPU's next_ip for the first
//!                                     instruction (= instr1_start +
//!                                     instr1_len)
//!   MODFORGE_DISP2_OFF    (required) byte offset of second disp32
//!   MODFORGE_DISP2_NEXT_IP (required) match-relative offset of
//!                                     CPU's next_ip for the second
//!                                     instruction
//!   MODFORGE_PAIR_DELTA   (optional) if set, only log hits where
//!                                    target2 - target1 == this
//!                                    signed value (decimal or hex)
//!   MODFORGE_MAX_HITS     (optional, default 256)
//!
//! Example: scan for set-1/set-0 byte pairs where target2 - target1
//! == -0x79 (debug-mode unlock block fingerprint):
//!   MODFORGE_SIG="c6 05 ?? ?? ?? ?? 01 c6 05 ?? ?? ?? ?? 00" \
//!   MODFORGE_DISP1_OFF=2 MODFORGE_DISP1_NEXT_IP=7 \
//!   MODFORGE_DISP2_OFF=9 MODFORGE_DISP2_NEXT_IP=14 \
//!   MODFORGE_PAIR_DELTA=-0x79 \

mod common;

use serde_json::json;

fn need(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}
fn parse_uint(s: &str) -> u64 {
    if let Some(stripped) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(stripped, 16).unwrap_or(0)
    } else {
        s.parse().unwrap_or(0)
    }
}
fn parse_signed(s: &str) -> i64 {
    let neg = s.starts_with('-');
    let body = if neg { &s[1..] } else { s };
    let m = parse_uint(body) as i64;
    if neg { -m } else { m }
}

#[test]
fn scan_and_decode_disp32_pair() {
    let Some(sig) = need("MODFORGE_SIG") else {
        eprintln!("skipping: set MODFORGE_SIG and the 4 offset env vars to use");
        return;
    };
    let d1_off = need("MODFORGE_DISP1_OFF")
        .map(|s| parse_uint(&s) as u32)
        .expect("MODFORGE_DISP1_OFF required");
    let d1_nip = need("MODFORGE_DISP1_NEXT_IP")
        .map(|s| parse_uint(&s) as u32)
        .expect("MODFORGE_DISP1_NEXT_IP required");
    let d2_off = need("MODFORGE_DISP2_OFF")
        .map(|s| parse_uint(&s) as usize)
        .expect("MODFORGE_DISP2_OFF required");
    let d2_nip = need("MODFORGE_DISP2_NEXT_IP")
        .map(|s| parse_uint(&s) as usize)
        .expect("MODFORGE_DISP2_NEXT_IP required");
    let pair_delta = need("MODFORGE_PAIR_DELTA").map(|s| parse_signed(&s));
    let max_hits = need("MODFORGE_MAX_HITS")
        .map(|s| parse_uint(&s) as u32)
        .unwrap_or(256);

    let Some(game) = common::launch("research_decode_disp32_pair") else {
        return;
    };

    // First pass: scan_all on the sig, anchored on disp1.
    let resp = game
        .op_json(
            "patterns.sleuth.scan_all",
            &json!({
                "sig": sig,
                "disp32_offset": d1_off,
                "instr_len": d1_nip,
                "context_bytes": (d2_nip as u32).max(16),
                "max_hits": max_hits,
            }),
        )
        .expect("patterns.sleuth.scan_all must succeed");
    let result = resp.get("result").expect("result");
    let hits = result
        .get("hits")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut accepted = 0usize;
    for h in &hits {
        let instr_str = h.get("instr_addr").and_then(|v| v.as_str()).unwrap_or("?");
        let instr = u64::from_str_radix(instr_str.trim_start_matches("0x"), 16).unwrap_or(0);
        let target1 = u64::from_str_radix(
            h.get("decoded_target")
                .and_then(|v| v.as_str())
                .unwrap_or("0x0")
                .trim_start_matches("0x"),
            16,
        )
        .unwrap_or(0);
        // Read enough bytes to cover disp2.
        let read_len = (d2_nip as u32).max(16);
        let bytes_resp = game
            .op_json(
                "patterns.read_bytes",
                &json!({"addr": format!("0x{instr:x}"), "n": read_len}),
            )
            .ok();
        let target2 = bytes_resp.and_then(|r| {
            let s = r
                .get("result")
                .and_then(|x| x.get("bytes"))
                .and_then(|v| v.as_str())?
                .to_string();
            let b: Vec<u8> = s
                .split_whitespace()
                .filter_map(|t| u8::from_str_radix(t, 16).ok())
                .collect();
            if b.len() < d2_off + 4 {
                return None;
            }
            let d2 = i32::from_le_bytes([b[d2_off], b[d2_off + 1], b[d2_off + 2], b[d2_off + 3]]);
            Some((instr.wrapping_add(d2_nip as u64) as i64).wrapping_add(d2 as i64) as u64)
        });
        let Some(t2) = target2 else {
            continue;
        };
        let delta = (t2 as i64).wrapping_sub(target1 as i64);
        let keep = pair_delta.map(|d| delta == d).unwrap_or(true);
        let marker = if pair_delta.map(|d| d == delta).unwrap_or(false) {
            "  <-- MATCH"
        } else {
            ""
        };
        if keep || pair_delta.is_none() {
            game.log().event(
                "DECODED",
                &format!("instr=0x{instr:x} t1=0x{target1:x} t2=0x{t2:x} delta={delta:#x}{marker}"),
            );
            if keep && pair_delta.is_some() {
                accepted += 1;
            }
        }
    }
    if let Some(d) = pair_delta {
        game.pass(&format!(
            "{} hits matched delta {d:#x} out of {} total",
            accepted,
            hits.len()
        ));
    } else {
        game.pass(&format!("decoded {} hits (no delta filter)", hits.len()));
    }
}
