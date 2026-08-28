//! Asset-file locator. Finds files next to the running game exe.

use crate::harness::RunningGame;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Config {
    pub asset_name: String,
    pub expect_path_suffix: Option<String>,
    pub expect_min_size: Option<u64>,
    /// Sub-directories to probe under the exe's parent. Default:
    /// `["data", ".", "../data", "../../data", "Content/data"]`.
    pub probe_dirs: Vec<String>,
}

impl Config {
    pub fn defaults(asset_name: impl Into<String>) -> Self {
        Self {
            asset_name: asset_name.into(),
            expect_path_suffix: None,
            expect_min_size: None,
            probe_dirs: vec![
                "data".into(),
                ".".into(),
                "../data".into(),
                "../../data".into(),
                "Content/data".into(),
            ],
        }
    }

    /// Parse from env vars. With `prefix="HORSEY_ASSET"`:
    /// - `HORSEY_ASSET_NAME` (default `genes.xml`)
    /// - `HORSEY_ASSET_EXPECT_PATH` (suffix match)
    /// - `HORSEY_ASSET_EXPECT_MIN_SIZE`
    pub fn from_env(prefix: &str, default_name: &str) -> Self {
        let name = std::env::var(format!("{prefix}_NAME")).unwrap_or_else(|_| default_name.into());
        let mut cfg = Self::defaults(name);
        cfg.expect_path_suffix = std::env::var(format!("{prefix}_EXPECT_PATH")).ok();
        cfg.expect_min_size = std::env::var(format!("{prefix}_EXPECT_MIN_SIZE"))
            .ok()
            .and_then(|s| s.parse().ok());
        cfg
    }
}

#[derive(Debug)]
pub struct AssetFound {
    pub path: PathBuf,
    pub size: u64,
}

pub fn run(game: &RunningGame, cfg: &Config) -> Result<AssetFound> {
    let bi = game
        .op_json("game.build_info", &json!({}))
        .map_err(|e| anyhow!("game.build_info: {e}"))?;
    let exe_path = bi
        .get("result")
        .and_then(|r| r.get("exe_path"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("build_info missing exe_path"))?
        .to_string();
    eprintln!("exe_path: {exe_path}");
    let exe = PathBuf::from(&exe_path);
    let parent: &Path = exe
        .parent()
        .ok_or_else(|| anyhow!("exe_path has no parent"))?;
    find_asset_in_dirs(parent, &cfg.asset_name, &cfg.probe_dirs, cfg)
}

/// Pure path-search logic: probe each `parent/dir/asset_name` and
/// return the first that exists. Verifies expectations against the
/// found path. Unit-testable without a game.
pub fn find_asset_in_dirs(
    parent: &Path,
    asset_name: &str,
    probe_dirs: &[String],
    cfg: &Config,
) -> Result<AssetFound> {
    let candidates: Vec<PathBuf> = probe_dirs
        .iter()
        .map(|d| parent.join(d).join(asset_name))
        .collect();
    let mut found: Option<PathBuf> = None;
    for c in &candidates {
        let exists = c.exists();
        eprintln!("  {} -> exists={exists}", c.display());
        if exists && found.is_none() {
            found = Some(c.clone());
        }
    }
    let found = found.ok_or_else(|| anyhow!("{asset_name} not found in any candidate location"))?;
    let size = std::fs::metadata(&found).map(|m| m.len()).unwrap_or(0);
    eprintln!("[FOUND] {} ({} bytes)", found.display(), size);

    if let Some(suffix) = &cfg.expect_path_suffix {
        let got = found.to_string_lossy().to_string();
        if !got.ends_with(suffix) && got != *suffix {
            return Err(anyhow!(
                "expect_path suffix '{suffix}' not matched by '{got}'"
            ));
        }
    }
    if let Some(min) = cfg.expect_min_size {
        if size < min {
            return Err(anyhow!(
                "asset {found:?} is {size} bytes, expected >= {min}"
            ));
        }
    }

    Ok(AssetFound { path: found, size })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_asset_in_dirs_returns_first_match() {
        let tmp = std::env::temp_dir().join(format!("modforge_assets_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("data")).unwrap();
        std::fs::create_dir_all(tmp.join("Content/data")).unwrap();
        let asset_in_data = tmp.join("data/genes.xml");
        std::fs::write(&asset_in_data, b"hello").unwrap();
        // Also place a different file deeper to verify "first match wins".
        let _ = std::fs::write(tmp.join("Content/data/genes.xml"), b"different");

        let probe = vec!["data".into(), ".".into(), "Content/data".into()];
        let cfg = Config::defaults("genes.xml");
        let r = find_asset_in_dirs(&tmp, "genes.xml", &probe, &cfg).unwrap();
        assert_eq!(r.path, asset_in_data);
        assert_eq!(r.size, 5);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn find_asset_in_dirs_errors_when_missing() {
        let tmp = std::env::temp_dir().join(format!("modforge_assets_miss_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let cfg = Config::defaults("absent.xml");
        let r = find_asset_in_dirs(&tmp, "absent.xml", &["data".to_string()], &cfg);
        assert!(r.is_err());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn find_asset_in_dirs_enforces_min_size() {
        let tmp = std::env::temp_dir().join(format!("modforge_assets_size_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("data")).unwrap();
        std::fs::write(tmp.join("data/x.bin"), b"abc").unwrap();
        let mut cfg = Config::defaults("x.bin");
        cfg.expect_min_size = Some(10);
        let r = find_asset_in_dirs(&tmp, "x.bin", &["data".into()], &cfg);
        assert!(r.is_err());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
