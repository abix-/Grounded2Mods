//! Function-entry verifier for native-PE x64 MSVC binaries.
//!
//! Two related primitives:
//! - `is_msvc_x64_prologue(&[u8]) -> bool`: pure byte matcher.
//! - `verify_fn_entry(&RunningGame, &Config) -> Result<Verified>`:
//!   read bytes around a runtime RVA, dump them, optionally assert
//!   the entry looks like a real function prologue.

use crate::harness::RunningGame;
use crate::research;
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};

/// Loose recognizer for common MSVC x64 function prologues. Returns
/// true when `b` starts with a byte sequence that is plausibly the
/// first bytes of a real function entry. Tuned conservatively: false
/// negatives (a real prologue we miss) are preferable to false
/// positives (mid-function code we accept).
pub fn is_msvc_x64_prologue(b: &[u8]) -> bool {
    if b.len() < 4 {
        return false;
    }
    // `mov [rsp+disp8], rbx` (48 89 5c 24 ..)
    if b[0] == 0x48 && b[1] == 0x89 && b[2] == 0x5c && b[3] == 0x24 {
        return true;
    }
    // `mov [rsp+disp8], rbp/rsi/rdi` (48 89 6c|74|7c 24 ..)
    if b[0] == 0x48 && b[1] == 0x89 && matches!(b[2], 0x6c | 0x74 | 0x7c) && b[3] == 0x24 {
        return true;
    }
    // `mov rax, rsp` then ModR/M (48 8b c4 ..). When followed by a
    // push (55/53/56/57) or REX.x push it's a tight-frame prologue.
    if b.len() >= 4 && b[0] == 0x48 && b[1] == 0x8b && b[2] == 0xc4 {
        return matches!(b[3], 0x55 | 0x53 | 0x56 | 0x57 | 0x40..=0x4f);
    }
    // `push rdi; push r12..r15` (57 41 54..57 ..)
    if b.len() >= 4 && b[0] == 0x57 && b[1] == 0x41 && matches!(b[2], 0x54..=0x57) {
        return matches!(b[3], 0x41 | 0x48 | 0x55 | 0x56 | 0x57);
    }
    // `push rbx; push rbp/rsi/rdi` (53 55|56|57)
    if b[0] == 0x53 && matches!(b[1], 0x55 | 0x56 | 0x57) {
        return true;
    }
    // `push rsi; push rdi; sub rsp, ...` (56 57 48 83|81 ec ..)
    if b.len() >= 4 && b[0] == 0x56 && b[1] == 0x57 && b[2] == 0x48 {
        return matches!(b[3], 0x83 | 0x81);
    }
    // `push r12..r15` (41 54..57)
    if b[0] == 0x41 && matches!(b[1], 0x54..=0x57) {
        return true;
    }
    // `push rbx/rbp/rsi/rdi` with REX prefix (40 53|55|56|57)
    if b[0] == 0x40 && matches!(b[1], 0x53 | 0x55 | 0x56 | 0x57) {
        return true;
    }
    // `sub rsp, imm8` (48 83 ec ..) and `sub rsp, imm32` (48 81 ec ..)
    if b[0] == 0x48 && b[1] == 0x83 && b[2] == 0xec {
        return true;
    }
    if b[0] == 0x48 && b[1] == 0x81 && b[2] == 0xec {
        return true;
    }
    false
}

/// Looser recognizer that admits any prologue starting with a recognized
/// REX-prefixed move/sub or a single-byte push. Used for "is this byte
/// position plausibly the start of SOME function?" checks where the
/// exact prologue shape doesn't matter (e.g. dryrun precheck before
/// arming a detour). Pairs with [`is_msvc_x64_prologue`] (the tighter
/// matcher) which expects specific 4-byte sequences.
pub fn is_msvc_x64_prologue_loose(b: &[u8]) -> bool {
    if b.len() < 2 {
        return false;
    }
    if b.iter().take(8).all(|&x| x == 0xcc) {
        return false;
    }
    if b.iter().take(8).all(|&x| x == 0x00) {
        return false;
    }
    match b[0] {
        0x48 => match b[1] {
            0x89 | 0x8b => true,
            0x83 => b.len() >= 3 && b[2] == 0xec,
            0x81 => b.len() >= 3 && b[2] == 0xec,
            0x57 | 0x55 | 0x53 | 0x56 => true,
            _ => false,
        },
        0x49 | 0x4c => matches!(b[1], 0x89 | 0x8b),
        0x44 => b[1] == 0x89,
        0x53 | 0x55 | 0x56 | 0x57 => true,
        0x41 => matches!(b[1], 0x54 | 0x55 | 0x56 | 0x57),
        0x40 => matches!(b[1], 0x53 | 0x55 | 0x56 | 0x57),
        _ => false,
    }
}

/// Walk back/forward through `.text` from `anchor` until 3+ consecutive
/// `0xcc` bytes (MSVC inter-function int3 padding). Returns the
/// padding-end-to-padding-end span enclosing `anchor`, or None if no
/// boundary was found within `+/- lookback` bytes.
pub fn find_fn_bounds_via_int3(
    game: &RunningGame,
    anchor: u64,
    lookback: usize,
) -> Result<Option<(u64, u64)>> {
    let lo = anchor.saturating_sub(lookback as u64);
    let total = lookback * 2;
    let mut bytes: Vec<u8> = Vec::with_capacity(total);
    let mut cursor = lo;
    while bytes.len() < total {
        let remaining = total - bytes.len();
        let take = remaining.min(4096) as u32;
        let chunk = research::read_bytes(game, cursor, take)
            .with_context(|| format!("read_bytes at 0x{cursor:x} n={take}"))?;
        if chunk.is_empty() {
            break;
        }
        bytes.extend_from_slice(&chunk);
        cursor = cursor.wrapping_add(chunk.len() as u64);
    }
    let anchor_off = (anchor - lo) as usize;
    if anchor_off >= bytes.len() {
        return Ok(None);
    }
    let start = walk_back_to_padding_end(&bytes, anchor_off);
    let end = walk_forward_to_padding(&bytes, anchor_off);
    match (start, end) {
        (Some(s), Some(e)) => Ok(Some((lo + s as u64, lo + e as u64))),
        _ => Ok(None),
    }
}

fn walk_back_to_padding_end(bytes: &[u8], from: usize) -> Option<usize> {
    if from < 3 {
        return None;
    }
    for off in (0..from.saturating_sub(2)).rev() {
        if bytes[off] == 0xcc && bytes[off + 1] == 0xcc && bytes[off + 2] == 0xcc {
            return Some(off + 3);
        }
    }
    None
}

fn walk_forward_to_padding(bytes: &[u8], from: usize) -> Option<usize> {
    if from + 3 > bytes.len() {
        return None;
    }
    for off in from..bytes.len().saturating_sub(2) {
        if bytes[off] == 0xcc && bytes[off + 1] == 0xcc && bytes[off + 2] == 0xcc {
            return Some(off);
        }
    }
    None
}

/// Config for `verify_fn_entry`.
#[derive(Debug, Clone)]
pub struct Config {
    pub rva: usize,
    pub lookbehind: usize,
    pub lookahead: usize,
    pub expect_prologue: bool,
}

impl Config {
    pub fn defaults() -> Self {
        Self {
            rva: 0,
            lookbehind: 16,
            lookahead: 64,
            expect_prologue: true,
        }
    }

    /// Parse from env vars with the given prefix. With `prefix="HORSEY_FN"`:
    /// - `HORSEY_FN_RVA=<hex>` (required)
    /// - `HORSEY_FN_LOOKBEHIND=<n>`
    /// - `HORSEY_FN_LOOKAHEAD=<n>`
    /// - `HORSEY_FN_EXPECT_PROLOGUE=<0|1>`
    pub fn from_env(prefix: &str) -> Result<Self> {
        let mut cfg = Self::defaults();
        let rva_key = format!("{prefix}_RVA");
        let rva_s = std::env::var(&rva_key).map_err(|_| anyhow!("{rva_key} required"))?;
        cfg.rva = parse_hex_or_dec(&rva_s).with_context(|| format!("{rva_key} parse"))?;
        if let Ok(s) = std::env::var(format!("{prefix}_LOOKBEHIND")) {
            cfg.lookbehind = s.parse().context("LOOKBEHIND parse")?;
        }
        if let Ok(s) = std::env::var(format!("{prefix}_LOOKAHEAD")) {
            cfg.lookahead = s.parse().context("LOOKAHEAD parse")?;
        }
        if let Ok(s) = std::env::var(format!("{prefix}_EXPECT_PROLOGUE")) {
            cfg.expect_prologue = s != "0";
        }
        Ok(cfg)
    }
}

/// Output of `verify_fn_entry`.
#[derive(Debug)]
pub struct Verified {
    pub image_base: u64,
    pub addr: u64,
    pub bytes: Vec<u8>,
    pub prologue_ok: bool,
}

/// Read bytes around a function RVA, dump to stderr, optionally assert
/// the first bytes at the RVA are a recognized MSVC prologue.
pub fn verify_fn_entry(game: &RunningGame, cfg: &Config) -> Result<Verified> {
    let bi = game
        .op_json("game.build_info", &json!({}))
        .map_err(|e| anyhow!("game.build_info: {e}"))?;
    let base_s = bi
        .get("result")
        .and_then(|r| r.get("image_base"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("image_base missing"))?;
    let image_base = u64::from_str_radix(base_s.trim_start_matches("0x"), 16)
        .with_context(|| "image_base parse")?;

    let total = cfg.lookbehind + cfg.lookahead;
    let addr = image_base + cfg.rva as u64 - cfg.lookbehind as u64;
    let bytes = research::read_bytes(game, addr, total as u32)
        .with_context(|| format!("read_bytes 0x{addr:x} n={total}"))?;
    if bytes.len() != total {
        return Err(anyhow!("short read: got {}, want {total}", bytes.len()));
    }

    eprintln!(
        "bytes around image_base+0x{:x} (-0x{:x}..+0x{:x}):",
        cfg.rva, cfg.lookbehind, cfg.lookahead
    );
    for (i, b) in bytes.iter().enumerate() {
        let off = i as isize - cfg.lookbehind as isize;
        if i % 16 == 0 {
            eprint!("\n+0x{off:+04x}: ");
        }
        eprint!("{b:02x} ");
    }
    eprintln!();

    let entry = &bytes[cfg.lookbehind..];
    let prologue_ok = is_msvc_x64_prologue(entry);
    if cfg.expect_prologue && !prologue_ok {
        return Err(anyhow!(
            "bytes at RVA 0x{:x} do not look like an MSVC prologue. first 16: {:02x?}",
            cfg.rva,
            &entry[..entry.len().min(16)]
        ));
    }

    Ok(Verified {
        image_base,
        addr: image_base + cfg.rva as u64,
        bytes,
        prologue_ok,
    })
}

pub fn parse_hex_or_dec(s: &str) -> Result<usize> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return usize::from_str_radix(h, 16).map_err(|e| anyhow!("hex parse: {e}"));
    }
    s.parse::<usize>().map_err(|e| anyhow!("dec parse: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loose_prologue_admits_more_prefixes() {
        // Loose matcher accepts shadow-space r12-r15 moves (4c 89 ..)
        // that the strict matcher rejects.
        assert!(is_msvc_x64_prologue_loose(&[0x4c, 0x89, 0x44, 0x24]));
        assert!(is_msvc_x64_prologue_loose(&[0x44, 0x89, 0x44, 0x24]));
        assert!(is_msvc_x64_prologue_loose(&[0x49, 0x89, 0x5c, 0x24]));
        // Strict matcher rejects these (only 48-prefixed stores).
        assert!(!is_msvc_x64_prologue(&[0x4c, 0x89, 0x44, 0x24]));
        assert!(!is_msvc_x64_prologue(&[0x44, 0x89, 0x44, 0x24]));
        // Both reject all-int3 and all-zero.
        assert!(!is_msvc_x64_prologue_loose(&[0xcc; 8]));
        assert!(!is_msvc_x64_prologue_loose(&[0x00; 8]));
        assert!(!is_msvc_x64_prologue(&[0xcc; 8]));
    }

    #[test]
    fn int3_padding_walkers() {
        // bytes: ... cc cc cc | real code | cc cc cc ... | more real code | cc cc cc
        let mut buf = vec![0xcc; 5];
        buf.extend_from_slice(&[0x48, 0x89, 0x5c, 0x24, 0x08]); // function A entry (5..10)
        buf.extend_from_slice(&[0xc3, 0xcc, 0xcc, 0xcc]); // ret + padding (10..14)
        buf.extend_from_slice(&[0x40, 0x53, 0x55, 0x56]); // function B entry (14..18)

        // anchor inside function A at offset 6 -> walk back finds padding-end at 5
        let end = walk_back_to_padding_end(&buf, 6);
        assert_eq!(end, Some(5));

        // walk forward from anchor inside A finds the int3 run starting at 11
        let fwd = walk_forward_to_padding(&buf, 6);
        assert_eq!(fwd, Some(11));

        // anchor in the very first bytes (no preceding padding) returns None
        assert_eq!(walk_back_to_padding_end(&buf, 2), None);
    }

    #[test]
    fn parse_hex_or_dec_accepts_both() {
        assert_eq!(parse_hex_or_dec("0x140").unwrap(), 0x140);
        assert_eq!(parse_hex_or_dec("320").unwrap(), 320);
        assert_eq!(parse_hex_or_dec("  0X1F  ").unwrap(), 0x1f);
        assert!(parse_hex_or_dec("xyz").is_err());
    }

    #[test]
    fn prologue_recognizes_common_msvc_starts() {
        // mov [rsp+disp], rbx
        assert!(is_msvc_x64_prologue(&[0x48, 0x89, 0x5c, 0x24, 0x08]));
        // push rbx; push rbp
        assert!(is_msvc_x64_prologue(&[0x53, 0x55, 0x00, 0x00]));
        // push r12
        assert!(is_msvc_x64_prologue(&[0x41, 0x54, 0x00, 0x00]));
        // sub rsp, imm8
        assert!(is_msvc_x64_prologue(&[0x48, 0x83, 0xec, 0x28]));
        // not a prologue
        assert!(!is_msvc_x64_prologue(&[0x90, 0x90, 0x90, 0x90])); // nops
        assert!(!is_msvc_x64_prologue(&[0xcc, 0xcc, 0xcc, 0xcc])); // int3
        // too short
        assert!(!is_msvc_x64_prologue(&[0x48, 0x89]));
    }
}
