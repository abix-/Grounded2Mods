//! `game.build_info` contract assertion.
//!
//! Most other testkit primitives pull image_base / exe_path from this
//! cmdlet, so it's load-bearing for the toolkit. This module locks
//! the response shape.

use crate::harness::RunningGame;
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};

#[derive(Debug)]
pub struct BuildInfo {
    pub image_base: u64,
    pub exe_path: String,
    pub exe_size: u64,
    pub image_sha256: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub expect_exe_size: Option<u64>,
    pub expect_exe_sha256: Option<String>,
    /// Substring the exe_path must contain (case-insensitive). Use as
    /// a crude "is this the right game?" check. e.g. `"horsey"`.
    pub expect_exe_path_contains: Option<String>,
}

impl Config {
    pub fn from_env(prefix: &str) -> Self {
        Self {
            expect_exe_size: std::env::var(format!("{prefix}_EXPECT_EXE_SIZE"))
                .ok()
                .map(|s| s.parse().expect("EXPECT_EXE_SIZE not a number")),
            expect_exe_sha256: std::env::var(format!("{prefix}_EXPECT_EXE_SHA256")).ok(),
            expect_exe_path_contains: std::env::var(format!("{prefix}_EXPECT_PATH_CONTAINS")).ok(),
        }
    }
}

/// Fetch + validate. Asserts default invariants (image_base in the
/// expected high range, plausible exe_size, exe_path non-empty) and
/// any caller-supplied expectations.
pub fn run(game: &RunningGame, cfg: &Config) -> Result<BuildInfo> {
    let r = game
        .op_json("game.build_info", &json!({}))
        .map_err(|e| anyhow!("game.build_info: {e}"))?;
    eprintln!("BUILD INFO: {r}");
    let result = r.get("result").ok_or_else(|| anyhow!("missing result"))?;

    let image_base_s = result
        .get("image_base")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("image_base missing or not string"))?;
    let image_base = u64::from_str_radix(image_base_s.trim_start_matches("0x"), 16)
        .context("image_base not hex")?;
    if image_base == 0 {
        return Err(anyhow!("image_base=0"));
    }
    if image_base < 0x1_0000_0000 {
        return Err(anyhow!(
            "image_base 0x{image_base:x} not in expected high 64-bit range"
        ));
    }

    let exe_path = result
        .get("exe_path")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("exe_path missing"))?
        .to_string();
    let exe_size = result
        .get("exe_size")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("exe_size missing"))?;
    if !(1_000_000..100_000_000).contains(&exe_size) {
        return Err(anyhow!("exe_size {exe_size} outside plausible range"));
    }
    let image_sha256 = result
        .get("image_sha256")
        .and_then(Value::as_str)
        .map(String::from);

    if let Some(needle) = &cfg.expect_exe_path_contains {
        if !exe_path
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase())
        {
            return Err(anyhow!(
                "exe_path '{exe_path}' doesn't contain expected substring '{needle}'"
            ));
        }
    }
    if let Some(want) = cfg.expect_exe_size {
        if exe_size != want {
            return Err(anyhow!("exe_size mismatch: got {exe_size} want {want}"));
        }
    }
    if let Some(want) = &cfg.expect_exe_sha256 {
        let got = image_sha256
            .as_deref()
            .ok_or_else(|| anyhow!("image_sha256 missing"))?;
        if got.to_ascii_lowercase() != want.to_ascii_lowercase() {
            return Err(anyhow!("image_sha256 mismatch"));
        }
    }

    Ok(BuildInfo {
        image_base,
        exe_path,
        exe_size,
        image_sha256,
    })
}
