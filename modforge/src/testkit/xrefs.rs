//! Cross-reference (xref) count assertion.
//!
//! Thin wrapper over `mem.find_xrefs` that asserts a minimum hit
//! count for a known target. Used to verify (a) the find_xrefs op
//! still works, (b) a given target is the heavily-referenced global
//! you think it is.

use crate::harness::RunningGame;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct Config {
    pub target_addr: u64,
    pub min_hits: usize,
}

impl Config {
    /// Parse from env vars. With `prefix="HORSEY_XREF"`:
    /// - `HORSEY_XREF_TARGET=<hex>` (required if no default supplied)
    /// - `HORSEY_XREF_MIN_HITS=<n>` (default `10`)
    pub fn from_env(prefix: &str, default_target: Option<u64>) -> Result<Self> {
        let target_addr = match std::env::var(format!("{prefix}_TARGET")) {
            Ok(s) => u64::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16)
                .map_err(|e| anyhow!("XREF_TARGET parse: {e}"))?,
            Err(_) => default_target.ok_or_else(|| anyhow!("{prefix}_TARGET required"))?,
        };
        let min_hits = std::env::var(format!("{prefix}_MIN_HITS"))
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        Ok(Self {
            target_addr,
            min_hits,
        })
    }
}

#[derive(Debug)]
pub struct XrefResult {
    pub target_addr: u64,
    pub hits: Vec<Value>,
}

pub fn run(game: &RunningGame, cfg: &Config) -> Result<XrefResult> {
    if cfg.target_addr == 0 {
        return Err(anyhow!("xref target null"));
    }
    let resp = game
        .op_json("mem.find_xrefs", &json!({"target_addr": cfg.target_addr}))
        .map_err(|e| anyhow!("mem.find_xrefs: {e}"))?;
    let hits = resp
        .get("result")
        .and_then(|r| r.get("hits"))
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| anyhow!("find_xrefs: result.hits not array. resp={resp}"))?;
    eprintln!("xref target=0x{:x} hits={}", cfg.target_addr, hits.len());
    if hits.len() < cfg.min_hits {
        return Err(anyhow!(
            "expected at least {} xrefs to target 0x{:x}; got {}",
            cfg.min_hits,
            cfg.target_addr,
            hits.len()
        ));
    }
    Ok(XrefResult {
        target_addr: cfg.target_addr,
        hits,
    })
}
