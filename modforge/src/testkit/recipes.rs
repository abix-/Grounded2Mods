//! Cross-game discovery recipes. Each combines several lower-level
//! research primitives into a complete "find X in any native-PE
//! binary" workflow.
//!
//! - `find_fn_by_rdata_string`: locate a function entry by scanning
//!   `.rdata` for a unique literal it references, then walking back
//!   through `.text` from each xref to the nearest int3-padded
//!   prologue. Universal MSVC technique.
//! - `find_struct_by_field_value`: locate a heap struct by scanning
//!   `.data` for a known u32 value (e.g. on-screen money), then
//!   verifying each candidate by reading adjacent fields. Universal.

use crate::harness::RunningGame;
use crate::research;
use crate::testkit::fn_entry;
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};

// ============================================================================
// find_fn_by_rdata_string
// ============================================================================

#[derive(Debug, Clone)]
pub struct FindFnByStringConfig {
    /// IDA-style hex bytes of the unique string literal (e.g. the
    /// hex of "retired %s (useless)").
    pub string_sig: String,
    /// Bytes to subtract from the string hit to get the format-string
    /// start (when the needle is a substring of a longer literal).
    /// Default 0.
    pub string_lookback: i64,
    /// How far back through `.text` to walk from each xref looking for
    /// the prologue.
    pub read_back: u64,
    /// Optional: assert that one of the candidate function entries
    /// resolves to this RVA.
    pub expect_fn_rva: Option<u64>,
}

impl FindFnByStringConfig {
    pub fn from_env(prefix: &str, default_sig: &str) -> Result<Self> {
        let string_sig =
            std::env::var(format!("{prefix}_STRING_HEX")).unwrap_or_else(|_| default_sig.into());
        let string_lookback: i64 = std::env::var(format!("{prefix}_STRING_LOOKBACK"))
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let read_back: u64 = std::env::var(format!("{prefix}_READ_BACK"))
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8192);
        let expect_fn_rva = std::env::var(format!("{prefix}_EXPECT_FN_RVA"))
            .ok()
            .map(|s| {
                u64::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16)
                    .expect("EXPECT_FN_RVA parse")
            });
        Ok(Self {
            string_sig,
            string_lookback,
            read_back,
            expect_fn_rva,
        })
    }
}

#[derive(Debug)]
pub struct FoundFn {
    pub xref_addr: u64,
    pub entry_addr: u64,
    pub entry_rva: u64,
}

pub fn find_fn_by_rdata_string(
    game: &RunningGame,
    cfg: &FindFnByStringConfig,
) -> Result<Vec<FoundFn>> {
    let r = game
        .op_json("targets.resolve.gamestate_ptr", &json!({}))
        .map_err(|e| anyhow!("targets.resolve.gamestate_ptr: {e}"))?;
    let ib = u64::from_str_radix(
        r.get("result")
            .and_then(|x| x.get("image_base"))
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("image_base missing"))?
            .trim_start_matches("0x"),
        16,
    )
    .context("image_base parse")?;
    eprintln!("image_base=0x{ib:x}");

    let str_hits = research::scan_rdata(game, &cfg.string_sig).context("scan_rdata")?;
    if str_hits.is_empty() {
        return Err(anyhow!("string sig not in .rdata"));
    }
    let str_addr = str_hits[0];
    eprintln!("string @ 0x{str_addr:x}");

    let target_addr = str_addr.wrapping_sub(cfg.string_lookback as u64);
    let xrefs = research::find_xrefs(game, target_addr).context("find_xrefs")?;
    if xrefs.is_empty() {
        return Err(anyhow!("no .text xrefs to string anchor 0x{target_addr:x}"));
    }

    let mut entries = Vec::new();
    for x in &xrefs {
        let instr = x.instr_addr;
        let lo = instr.saturating_sub(cfg.read_back);
        let mut bytes: Vec<u8> = Vec::with_capacity(cfg.read_back as usize);
        let mut cursor = lo;
        while (bytes.len() as u64) < cfg.read_back {
            let remaining = cfg.read_back - bytes.len() as u64;
            let take = remaining.min(4096) as u32;
            let chunk = match research::read_bytes(game, cursor, take) {
                Ok(c) => c,
                Err(_) => break,
            };
            if chunk.is_empty() {
                break;
            }
            bytes.extend_from_slice(&chunk);
            cursor = cursor.wrapping_add(chunk.len() as u64);
        }

        // Find last int3 padding run.
        let mut padding_end: Option<usize> = None;
        for off in (0..bytes.len().saturating_sub(3)).rev() {
            if bytes[off] == 0xcc && bytes[off + 1] == 0xcc && bytes[off + 2] == 0xcc {
                padding_end = Some(off + 3);
                break;
            }
        }
        let scan_from = padding_end.unwrap_or(0);
        let mut entry_off: Option<usize> = None;
        for off in scan_from..bytes.len().saturating_sub(6) {
            if bytes[off] == 0xcc {
                continue;
            }
            if fn_entry::is_msvc_x64_prologue(&bytes[off..]) {
                entry_off = Some(off);
                break;
            }
        }
        if let Some(off) = entry_off {
            let entry_addr = lo + off as u64;
            let entry_rva = 0x140000000_u64 + entry_addr.wrapping_sub(ib);
            eprintln!("xref@0x{instr:x} -> entry=0x{entry_addr:x} rva=0x{entry_rva:x}");
            entries.push(FoundFn {
                xref_addr: instr,
                entry_addr,
                entry_rva,
            });
        } else {
            eprintln!("xref@0x{instr:x} -> no prologue after cc-padding");
        }
    }

    if let Some(want) = cfg.expect_fn_rva {
        if !entries.iter().any(|e| e.entry_rva == want) {
            return Err(anyhow!(
                "EXPECT_FN_RVA=0x{want:x} not in candidates: {:?}",
                entries
                    .iter()
                    .map(|e| format!("0x{:x}", e.entry_rva))
                    .collect::<Vec<_>>()
            ));
        }
    }

    Ok(entries)
}

// ============================================================================
// find_struct_by_field_value
// ============================================================================

/// Verifier for a candidate base address. Reads adjacent fields via
/// `mem.peek`. Returns true if the candidate looks plausible.
pub type StructVerifier = Box<dyn Fn(&RunningGame, u64) -> bool>;

#[derive(Debug, Clone)]
pub struct FindStructConfig {
    /// The known field value to scan `.data` for.
    pub field_value: u32,
    /// Offset of the field inside the struct (used to compute the
    /// struct base when a candidate hits).
    pub field_offset: usize,
}

#[derive(Debug)]
pub struct FoundStruct {
    pub field_addr: u64,
    pub struct_base: u64,
}

/// Scan `.data` for `cfg.field_value`, call `verifier(struct_base)` on
/// each candidate, return the plausible set. The caller's verifier is
/// the game-specific layer (it knows what other fields to read and
/// what plausible ranges are).
pub fn find_struct_by_field_value(
    game: &RunningGame,
    cfg: &FindStructConfig,
    verifier: StructVerifier,
) -> Result<Vec<FoundStruct>> {
    let scan = game
        .op_json(
            "mem.scan_data",
            &json!({
                "value": cfg.field_value,
                "kind": "u32",
            }),
        )
        .map_err(|e| anyhow!("mem.scan_data: {e}"))?;
    let hits = scan
        .get("result")
        .and_then(|r| r.get("hits"))
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| anyhow!("scan_data: no hits array"))?;
    eprintln!(
        "scan_data: {} candidate(s) for u32 == {}",
        hits.len(),
        cfg.field_value
    );
    if hits.is_empty() {
        return Err(anyhow!("no .data u32 holds {}", cfg.field_value));
    }

    let mut plausible = Vec::new();
    for h in &hits {
        let field_addr = h
            .get("addr")
            .and_then(Value::as_str)
            .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .ok_or_else(|| anyhow!("hit missing addr"))?;
        let struct_base = field_addr.saturating_sub(cfg.field_offset as u64);
        if verifier(game, struct_base) {
            eprintln!("  OK candidate 0x{struct_base:x} (field at 0x{field_addr:x})");
            plausible.push(FoundStruct {
                field_addr,
                struct_base,
            });
        } else {
            eprintln!("  ?? rejected 0x{struct_base:x}");
        }
    }

    Ok(plausible)
}
