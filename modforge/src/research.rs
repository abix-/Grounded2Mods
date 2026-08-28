//! Binary-research workflow library.
//!
//! Reusable recipes for locating game-binary constants from a
//! running game over HTTP. Patternsleuth-backed: every scan goes
//! through `modforge::patterns::sleuth` via the per-mod HTTP ops
//! (`mem.scan_rdata`, `mem.find_xrefs`, `patterns.sleuth.scan_all`,
//! `patterns.read_bytes`). Hosts call these from harness tests
//! against any game (Horsey, Outworld, Endless, etc.).
//!
//! Available recipes (high-level R4 work):
//!
//! - [`read_bytes`]: chunked `patterns.read_bytes` covering > 4 KiB.
//! - [`scan_all`]: parsed wrapper around `patterns.sleuth.scan_all`.
//! - [`scan_in_window`]: scan + filter to a `[start, start+len)` range.
//! - [`scan_rdata`]: wrapper around `mem.scan_rdata`.
//! - [`find_xrefs`]: parsed wrapper around `mem.find_xrefs`.
//! - [`decode_field_offset_via_string`]: end-to-end YEAR-style field
//!   finder. Scans `.rdata` for a literal, xrefs to its `lea`,
//!   decodes a field-load instruction's `disp`.
//! - [`decode_imm_at_call_site`]: histogram immediates passed to a
//!   target function (struct sizes, arg constants).
//!
//! The per-mod tests in `horsey-mod/tests/research_*.rs` parse
//! env vars and call into these functions.

use crate::harness::RunningGame;
use crate::patterns::sleuth;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// One match returned by [`scan_all`] / [`scan_in_window`].
#[derive(Debug, Clone)]
pub struct ScanHit {
    pub instr_addr: u64,
    pub decoded_target: u64,
    pub disp32: i64,
    pub context: Vec<u8>,
}

/// One xref returned by [`find_xrefs`].
#[derive(Debug, Clone)]
pub struct XrefHit {
    pub instr_addr: u64,
    pub opcode_class: String,
    pub context: Vec<u8>,
}

fn parse_hex_bytes(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .filter_map(|t| u8::from_str_radix(t, 16).ok())
        .collect()
}

fn u64_from_hex(s: &str) -> u64 {
    u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)
}

/// `patterns.read_bytes` caps at 4 KiB per call. This wrapper
/// chunks any request larger than that.
pub fn read_bytes(game: &RunningGame, addr: u64, n: u32) -> Result<Vec<u8>> {
    const CHUNK: u32 = 4096;
    let mut out: Vec<u8> = Vec::with_capacity(n as usize);
    let mut cursor = addr;
    let mut remaining = n;
    while remaining > 0 {
        let take = remaining.min(CHUNK);
        let resp = game
            .op_json(
                "patterns.read_bytes",
                &json!({"addr": format!("0x{cursor:x}"), "n": take}),
            )
            .context("patterns.read_bytes failed")?;
        let s = resp
            .get("result")
            .and_then(|r| r.get("bytes"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let chunk = parse_hex_bytes(s);
        if chunk.is_empty() {
            break;
        }
        out.extend_from_slice(&chunk);
        cursor = cursor.wrapping_add(chunk.len() as u64);
        remaining = remaining.saturating_sub(chunk.len() as u32);
    }
    Ok(out)
}

/// Read `size` bytes (1/2/4/8) at `addr` and decode as an unsigned
/// little-endian integer.
pub fn read_uint(game: &RunningGame, addr: u64, size: usize) -> Result<u64> {
    anyhow::ensure!(matches!(size, 1 | 2 | 4 | 8), "size must be 1/2/4/8");
    let b = read_bytes(game, addr, size as u32)?;
    anyhow::ensure!(b.len() == size, "short read at 0x{addr:x}");
    Ok(b.iter().rev().fold(0u64, |a, &b| (a << 8) | b as u64))
}

/// Same as [`read_uint`] but sign-extends from `size` bytes.
pub fn read_int(game: &RunningGame, addr: u64, size: usize) -> Result<i64> {
    let b = read_bytes(game, addr, size as u32)?;
    Ok(match size {
        1 => (b[0] as i8) as i64,
        2 => i16::from_le_bytes([b[0], b[1]]) as i64,
        4 => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64,
        8 => i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
        _ => anyhow::bail!("size must be 1/2/4/8"),
    })
}

/// Scan all of `.text` for `sig`; each hit's disp32 (at
/// `disp32_offset` within the match) is decoded relative to
/// `match_start + instr_len`.
pub fn scan_all(
    game: &RunningGame,
    sig: &str,
    disp32_offset: u32,
    instr_len: u32,
    context_bytes: u32,
    max_hits: u32,
) -> Result<Vec<ScanHit>> {
    let resp = game
        .op_json(
            "patterns.sleuth.scan_all",
            &json!({
                "sig": sig,
                "disp32_offset": disp32_offset,
                "instr_len": instr_len,
                "context_bytes": context_bytes,
                "max_hits": max_hits,
            }),
        )
        .context("patterns.sleuth.scan_all failed")?;
    let arr = resp
        .get("result")
        .and_then(|r| r.get("hits"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(arr
        .into_iter()
        .map(|h| ScanHit {
            instr_addr: u64_from_hex(h.get("instr_addr").and_then(Value::as_str).unwrap_or("0x0")),
            decoded_target: u64_from_hex(
                h.get("decoded_target")
                    .and_then(Value::as_str)
                    .unwrap_or("0x0"),
            ),
            disp32: {
                let s = h.get("disp32").and_then(Value::as_str).unwrap_or("0x0");
                let neg = s.starts_with('-');
                let body = if neg { &s[1..] } else { s };
                let m = u64_from_hex(body) as i64;
                if neg { -m } else { m }
            },
            context: parse_hex_bytes(h.get("context_hex").and_then(Value::as_str).unwrap_or("")),
        })
        .collect())
}

/// [`scan_all`] result filtered to hits with `instr_addr` in
/// `[start, start + len)`.
pub fn scan_in_window(
    game: &RunningGame,
    sig: &str,
    disp32_offset: u32,
    instr_len: u32,
    start: u64,
    len: u64,
    max_hits: u32,
) -> Result<Vec<ScanHit>> {
    let all = scan_all(game, sig, disp32_offset, instr_len, 16, max_hits)?;
    Ok(all
        .into_iter()
        .filter(|h| h.instr_addr >= start && h.instr_addr < start + len)
        .collect())
}

/// `mem.scan_rdata` wrapper: search `.rdata` for raw bytes.
pub fn scan_rdata(game: &RunningGame, sig: &str) -> Result<Vec<u64>> {
    let resp = game
        .op_json("mem.scan_rdata", &json!({"sig": sig}))
        .context("mem.scan_rdata failed")?;
    let arr = resp
        .get("result")
        .and_then(|r| r.get("hits"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(arr
        .into_iter()
        .filter_map(|h| h.get("addr").and_then(Value::as_str).map(u64_from_hex))
        .collect())
}

/// `mem.find_xrefs` wrapper: find RIP-relative references to a
/// target address in `.text`.
pub fn find_xrefs(game: &RunningGame, target_addr: u64) -> Result<Vec<XrefHit>> {
    let resp = game
        .op_json("mem.find_xrefs", &json!({"target_addr": target_addr}))
        .context("mem.find_xrefs failed")?;
    let arr = resp
        .get("result")
        .and_then(|r| r.get("hits"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(arr
        .into_iter()
        .map(|h| XrefHit {
            instr_addr: u64_from_hex(h.get("instr_addr").and_then(Value::as_str).unwrap_or("0x0")),
            opcode_class: h
                .get("opcode_class")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            context: parse_hex_bytes(h.get("context_hex").and_then(Value::as_str).unwrap_or("")),
        })
        .collect())
}

/// R4 end-to-end: find a struct-field offset by anchoring on a
/// `.rdata` string literal that's referenced by the function which
/// accesses the field.
///
/// Recipe (e.g. YEAR offset from pause-menu printf):
///   1. Scan `.rdata` for `string_hex`.
///   2. Adjust by `lea_offset` (signed): the actual lea-target in
///      `.text` may differ from the byte where our anchor matched
///      (e.g. MSVC's lea points 3 bytes before "retired" because
///      the full format string starts with "%s ").
///   3. find_xrefs against the lea target.
///   4. For each xref site, scan `+/- window` bytes for
///      `disp_opcode` (e.g. `8b 81` for `mov eax, [rcx+disp32]`)
///      and decode the disp at `disp_off` (1 or 4 bytes).
///   5. Return a histogram of decoded disps.
///
/// Caller picks the top value (usually the field offset).
pub fn decode_field_offset_via_string(
    game: &RunningGame,
    string_hex: &str,
    lea_offset: i64,
    disp_opcode: &str,
    disp_off: usize,
    disp_size: usize,
    window: u64,
) -> Result<BTreeMap<i64, usize>> {
    anyhow::ensure!(matches!(disp_size, 1 | 4), "disp_size must be 1 or 4");
    let rdata_hits = scan_rdata(game, string_hex)?;
    anyhow::ensure!(
        !rdata_hits.is_empty(),
        "string {string_hex:?} not in .rdata"
    );
    let str_addr = rdata_hits[0];
    let lea_target = str_addr.wrapping_add_signed(lea_offset);
    let xrefs = find_xrefs(game, lea_target)?;
    anyhow::ensure!(!xrefs.is_empty(), "no .text xrefs to 0x{lea_target:x}");

    let opcode_bytes = parse_hex_bytes(disp_opcode);
    let mut hist: BTreeMap<i64, usize> = BTreeMap::new();
    for x in xrefs {
        let lo = x.instr_addr.saturating_sub(window);
        let total = (window * 2) as u32;
        let bytes = read_bytes(game, lo, total)?;
        if bytes.len() < opcode_bytes.len() + disp_off + disp_size {
            continue;
        }
        for off in 0..=bytes
            .len()
            .saturating_sub(opcode_bytes.len() + disp_off + disp_size)
        {
            if bytes[off..off + opcode_bytes.len()] == opcode_bytes[..] {
                let d = off + disp_off;
                if d + disp_size > bytes.len() {
                    break;
                }
                let disp: i64 = if disp_size == 1 {
                    bytes[d] as i8 as i64
                } else {
                    i32::from_le_bytes([bytes[d], bytes[d + 1], bytes[d + 2], bytes[d + 3]]) as i64
                };
                *hist.entry(disp).or_insert(0) += 1;
            }
        }
    }
    Ok(hist)
}

// =============================================================================
// In-process (DLL-side) variants of the R4 workflows.
//
// The functions above route through the per-mod HTTP ops for use
// from harness tests. The functions below run the same algorithms
// using `modforge::patterns::sleuth` directly + raw pointer reads.
// They're for PRODUCTION resolvers inside an injected DLL where
// the harness isn't available.
// =============================================================================

/// In-process equivalent of [`decode_field_offset_via_string`].
///
/// SAFETY: caller is inside the injected DLL; all addresses are
/// inside the loaded image and live for the process lifetime.
/// Returns a histogram of decoded disp values (caller picks top).
pub fn in_process_decode_field_offset_via_string(
    string_hex: &str,
    lea_offset: i64,
    disp_opcode: &str,
    disp_off: usize,
    disp_size: usize,
    window: u64,
) -> Result<BTreeMap<i64, usize>> {
    anyhow::ensure!(matches!(disp_size, 1 | 4), "disp_size must be 1 or 4");
    let rdata_hits = sleuth::scan_rdata_matches(string_hex)?;
    anyhow::ensure!(
        !rdata_hits.is_empty(),
        "string {string_hex:?} not in .rdata"
    );
    let str_addr = rdata_hits[0] as u64;
    let lea_target = str_addr.wrapping_add_signed(lea_offset);
    // Find xrefs via the same opcode-prefix sweep as `mem.find_xrefs`.
    // For brevity, we use just the `lea` variants here (string-load).
    let lea_sigs: &[&str] = &[
        "48 8d 05", "48 8d 0d", "48 8d 15", "48 8d 1d", "48 8d 2d", "48 8d 35", "48 8d 3d",
    ];
    let mut xref_sites: Vec<u64> = Vec::new();
    for prefix in lea_sigs {
        let sig = format!("{prefix} X0x{lea_target:x}");
        if let Ok(addrs) = sleuth::scan_all_matches(&sig) {
            for a in addrs {
                xref_sites.push(a as u64);
            }
        }
    }
    anyhow::ensure!(
        !xref_sites.is_empty(),
        "no .text lea xrefs to 0x{lea_target:x}"
    );

    let opcode_bytes = parse_hex_bytes(disp_opcode);
    let mut hist: BTreeMap<i64, usize> = BTreeMap::new();
    for instr_addr in xref_sites {
        let lo = instr_addr.saturating_sub(window);
        let total = window * 2;
        // SAFETY: lo+total is inside .text; reads are safe.
        let bytes: &[u8] = unsafe { std::slice::from_raw_parts(lo as *const u8, total as usize) };
        if bytes.len() < opcode_bytes.len() + disp_off + disp_size {
            continue;
        }
        for off in 0..=bytes
            .len()
            .saturating_sub(opcode_bytes.len() + disp_off + disp_size)
        {
            if bytes[off..off + opcode_bytes.len()] == opcode_bytes[..] {
                let d = off + disp_off;
                if d + disp_size > bytes.len() {
                    break;
                }
                let disp: i64 = if disp_size == 1 {
                    bytes[d] as i8 as i64
                } else {
                    i32::from_le_bytes([bytes[d], bytes[d + 1], bytes[d + 2], bytes[d + 3]]) as i64
                };
                *hist.entry(disp).or_insert(0) += 1;
            }
        }
    }
    Ok(hist)
}

/// Pick the most-frequent value from a histogram. Returns None
/// for an empty histogram. Ties broken by lowest value.
pub fn histogram_top(hist: &BTreeMap<i64, usize>) -> Option<i64> {
    hist.iter()
        .max_by_key(|&(_, count)| *count)
        .map(|(&val, _)| val)
}

/// In-process: scan `.text` for a sig containing TWO disp32
/// operands, decode both, filter to matches where
/// `disp2 - disp1 == expected_delta`. Returns a histogram of
/// `disp1` values among accepted matches.
///
/// Use for "find a pair of consecutive struct-field accesses
/// where the offsets are a known distance apart" recipes
/// (HORSES_BEGIN/END, lo/hi pointer pairs, etc.).
pub fn in_process_decode_disp_pair_with_delta(
    sig: &str,
    disp1_offset: usize,
    disp2_offset: usize,
    disp_size: usize,
    expected_delta: i64,
) -> Result<BTreeMap<i64, usize>> {
    anyhow::ensure!(matches!(disp_size, 1 | 4), "disp_size must be 1 or 4");
    let hits = sleuth::scan_all_matches(sig)?;
    let mut hist: BTreeMap<i64, usize> = BTreeMap::new();
    for instr_addr in hits {
        let p1 = (instr_addr as u64).wrapping_add(disp1_offset as u64);
        let p2 = (instr_addr as u64).wrapping_add(disp2_offset as u64);
        // SAFETY: instr_addr is inside .text, mapped.
        let d1: i64 = unsafe {
            if disp_size == 1 {
                (p1 as *const i8).read_unaligned() as i64
            } else {
                (p1 as *const i32).read_unaligned() as i64
            }
        };
        let d2: i64 = unsafe {
            if disp_size == 1 {
                (p2 as *const i8).read_unaligned() as i64
            } else {
                (p2 as *const i32).read_unaligned() as i64
            }
        };
        if d2 - d1 == expected_delta {
            *hist.entry(d1).or_insert(0) += 1;
        }
    }
    Ok(hist)
}

/// In-process: scan `.text` for `sig` and return matches whose
/// address is in `[window_start, window_start + window_len)`.
/// Patternsleuth-backed.
pub fn in_process_scan_in_window(
    sig: &str,
    window_start: u64,
    window_len: u64,
) -> Result<Vec<u64>> {
    let all = sleuth::scan_all_matches(sig)?;
    Ok(all
        .into_iter()
        .map(|a| a as u64)
        .filter(|&a| a >= window_start && a < window_start + window_len)
        .collect())
}

/// In-process: decode an immediate operand inside an instruction
/// matching `opcode_prefix` somewhere in `[window_start, window_start
/// + window_len)`. Returns a histogram of decoded imms (caller
/// picks top, usually unique).
///
/// Use to recover constants embedded in instruction immediates:
/// struct sizes (`mov ecx, 0x498`), field offsets (`add rcx,
/// 0x2b8`), thresholds (`cmp eax, 5`), etc.
pub fn in_process_decode_imm_in_window(
    opcode_prefix: &str,
    imm_offset: usize,
    imm_size: usize,
    window_start: u64,
    window_len: u64,
) -> Result<BTreeMap<i64, usize>> {
    anyhow::ensure!(matches!(imm_size, 1 | 4), "imm_size must be 1 or 4");
    // Build a full sig with imm wildcards: `<prefix> ?? ?? ?? ??`
    // for imm_size=4 etc. patternsleuth needs explicit wildcards.
    let mut sig = opcode_prefix.to_string();
    // The opcode_prefix may end without trailing wildcards.
    // Append imm_size wildcards so patternsleuth scans the full
    // instruction width. imm_offset is the position WITHIN the
    // matched bytes (not within the instruction beyond the match).
    let prefix_bytes = parse_hex_bytes(opcode_prefix).len();
    let total_match_len = imm_offset + imm_size;
    if total_match_len > prefix_bytes {
        for _ in prefix_bytes..total_match_len {
            sig.push_str(" ??");
        }
    }
    let hits = in_process_scan_in_window(&sig, window_start, window_len)?;
    let mut hist: BTreeMap<i64, usize> = BTreeMap::new();
    for instr_addr in hits {
        // SAFETY: instr_addr is inside .text, mapped. The imm
        // may be misaligned in the byte stream so use
        // read_unaligned.
        let imm_addr = instr_addr.wrapping_add(imm_offset as u64);
        let val: i64 = unsafe {
            if imm_size == 1 {
                (imm_addr as *const i8).read_unaligned() as i64
            } else {
                (imm_addr as *const i32).read_unaligned() as i64
            }
        };
        *hist.entry(val).or_insert(0) += 1;
    }
    Ok(hist)
}

/// R4: for every `e8 X<target_fn>` (rel32 call to a known
/// function), walk backward `lookback` bytes for `imm_opcode`
/// (e.g. `b9` for `mov ecx, imm32`) and decode the immediate.
/// Returns a histogram of decoded imms. Useful for surfacing
/// struct sizes passed to allocators.
pub fn decode_imm_at_call_site(
    game: &RunningGame,
    target_fn: u64,
    imm_opcode: &str,
    imm_off: usize,
    imm_size: usize,
    lookback: u32,
) -> Result<BTreeMap<i64, usize>> {
    anyhow::ensure!(matches!(imm_size, 1 | 4), "imm_size must be 1 or 4");
    let sig = format!("e8 X0x{target_fn:x}");
    let calls = scan_all(game, &sig, 1, 5, 16, 4096)?;
    let opcode_bytes = parse_hex_bytes(imm_opcode);
    let mut hist: BTreeMap<i64, usize> = BTreeMap::new();
    for c in calls {
        let lo = c.instr_addr.saturating_sub(lookback as u64);
        let bytes = read_bytes(game, lo, lookback)?;
        if bytes.len() < opcode_bytes.len() + imm_off + imm_size {
            continue;
        }
        let max_off = bytes.len() - (opcode_bytes.len() + imm_off + imm_size);
        for off in (0..=max_off).rev() {
            if bytes[off..off + opcode_bytes.len()] == opcode_bytes[..] {
                let d = off + imm_off;
                let val: i64 = if imm_size == 1 {
                    bytes[d] as i8 as i64
                } else {
                    i32::from_le_bytes([bytes[d], bytes[d + 1], bytes[d + 2], bytes[d + 3]]) as i64
                };
                *hist.entry(val).or_insert(0) += 1;
                break;
            }
        }
    }
    Ok(hist)
}
