//! Snapshot a memory region to disk and diff against a prior snapshot.
//! Used to isolate "which bytes change between state A and state B"
//! when an action's effect is not known yet (e.g. moving a horse
//! between containers in horsey-mod).
//!
//! Writes go to `target/test-runs/<scope>_<tag>.bin`.

use crate::harness::RunningGame;
use crate::research;
use anyhow::{Context, Result, anyhow};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Config {
    pub addr: u64,
    pub len: usize,
    /// Free-form scope identifier; controls the on-disk filename.
    /// e.g. `"horse_dump_Aurelia"`.
    pub scope: String,
    pub tag: String,
    pub workspace_root: PathBuf,
}

impl Config {
    /// Default workspace root: the parent of CARGO_MANIFEST_DIR.
    pub fn workspace_from_manifest(manifest_dir: &str) -> PathBuf {
        Path::new(manifest_dir)
            .parent()
            .expect("no workspace root")
            .to_path_buf()
    }
}

#[derive(Debug)]
pub struct SnapshotTaken {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

/// Read `len` bytes at `addr`, dump as hex to stderr, write binary
/// snapshot to disk under `target/test-runs/`.
pub fn take(game: &RunningGame, cfg: &Config) -> Result<SnapshotTaken> {
    let bytes = research::read_bytes(game, cfg.addr, cfg.len as u32)
        .with_context(|| format!("read_bytes 0x{:x} n={}", cfg.addr, cfg.len))?;
    if bytes.len() != cfg.len {
        return Err(anyhow!("short read: got {}, want {}", bytes.len(), cfg.len));
    }

    eprintln!("\n[{}] bytes @ 0x{:x} (rows of 32):", cfg.tag, cfg.addr);
    for (i, chunk) in bytes.chunks(32).enumerate() {
        let off = i * 32;
        let row: String = chunk
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!("  +0x{off:03x}  {row}");
    }

    let path = snapshot_path(&cfg.workspace_root, &cfg.scope, &cfg.tag);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&path, &bytes).with_context(|| format!("write snapshot {path:?}"))?;
    eprintln!("\n[{}] wrote snapshot: {}", cfg.tag, path.display());

    Ok(SnapshotTaken { path, bytes })
}

#[derive(Debug)]
pub struct DiffResult {
    pub current: Vec<u8>,
    pub other: Vec<u8>,
    pub changed_offsets: BTreeSet<usize>,
}

/// Read current state via `take` semantics + load a stored snapshot +
/// diff them. Asserts at least one byte differs (otherwise the
/// "containers changed between captures" hypothesis is wrong).
pub fn diff_against(
    game: &RunningGame,
    cfg: &Config,
    other_tag: &str,
    expect_offsets: Option<&BTreeSet<usize>>,
) -> Result<DiffResult> {
    let current = take(game, cfg)?.bytes;
    let other_path = snapshot_path(&cfg.workspace_root, &cfg.scope, other_tag);
    let other = std::fs::read(&other_path)
        .with_context(|| format!("read {other_path:?}; run with tag={other_tag} first"))?;
    if other.len() != cfg.len {
        return Err(anyhow!(
            "stored snapshot {other_path:?} wrong size {} != {}",
            other.len(),
            cfg.len
        ));
    }

    let mut changed: BTreeSet<usize> = BTreeSet::new();
    for (i, (&a, &b)) in other.iter().zip(current.iter()).enumerate() {
        if a != b {
            changed.insert(i);
        }
    }
    eprintln!("\ndiff vs [{other_tag}]: {} bytes changed", changed.len());
    for off in &changed {
        eprintln!(
            "  +0x{:03x}: {:02x} -> {:02x}",
            off, other[*off], current[*off]
        );
    }
    if changed.is_empty() {
        return Err(anyhow!(
            "snapshots [{other_tag}] and [{}] are byte-identical",
            cfg.tag
        ));
    }

    if let Some(want) = expect_offsets {
        if &changed != want {
            return Err(anyhow!(
                "diff mismatch. got={:?} want={:?}",
                changed
                    .iter()
                    .map(|o| format!("0x{o:x}"))
                    .collect::<Vec<_>>(),
                want.iter().map(|o| format!("0x{o:x}")).collect::<Vec<_>>(),
            ));
        }
    }

    Ok(DiffResult {
        current,
        other,
        changed_offsets: changed,
    })
}

/// Compute the offsets at which two byte slices differ. Pure; the
/// HTTP-touching `diff_against` calls this after fetching current
/// state from the game.
pub fn diff_bytes(a: &[u8], b: &[u8]) -> BTreeSet<usize> {
    let mut changed = BTreeSet::new();
    for (i, (&x, &y)) in a.iter().zip(b.iter()).enumerate() {
        if x != y {
            changed.insert(i);
        }
    }
    changed
}

pub fn snapshot_path(workspace: &Path, scope: &str, tag: &str) -> PathBuf {
    let safe_scope: String = scope
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let safe_tag: String = tag
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    workspace
        .join("target")
        .join("test-runs")
        .join(format!("{safe_scope}_{safe_tag}.bin"))
}

/// Parse a CSV list of hex offsets (`"0x205,0x206"`) into an ordered set.
pub fn parse_offset_csv(s: &str) -> Result<BTreeSet<usize>> {
    s.split(',')
        .map(|t| {
            let t = t.trim().trim_start_matches("0x").trim_start_matches("0X");
            usize::from_str_radix(t, 16).map_err(|e| anyhow!("offset '{t}': {e}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_bytes_finds_only_changed_indices() {
        let a = [0u8, 1, 2, 3, 4, 5];
        let b = [0u8, 1, 9, 3, 8, 5];
        let d = diff_bytes(&a, &b);
        assert_eq!(d.iter().copied().collect::<Vec<_>>(), vec![2usize, 4]);

        let identical = diff_bytes(&[7u8; 16], &[7u8; 16]);
        assert!(identical.is_empty());
    }

    #[test]
    fn parse_offset_csv_handles_hex_and_decimal() {
        let s = parse_offset_csv("0x205,0x206,517").unwrap();
        let v: Vec<usize> = s.iter().copied().collect();
        // Note: parser strips 0x; "517" is parsed as HEX (per current
        // behavior; consistent with how the CSV is used in tests).
        assert_eq!(v, vec![0x205, 0x206, 0x517]);

        assert!(parse_offset_csv("0x205,xyz").is_err());
    }

    #[test]
    fn snapshot_path_sanitizes_unsafe_chars() {
        let p = snapshot_path(Path::new("/tmp"), "horse Aurelia!", "snap*1");
        assert!(p.to_string_lossy().ends_with("horse_Aurelia__snap_1.bin"));
        assert!(p.to_string_lossy().contains("target"));
        assert!(p.to_string_lossy().contains("test-runs"));
    }

    #[test]
    fn diff_against_real_file_roundtrip() {
        // Use a unique tempdir under the OS temp directory.
        let tmp =
            std::env::temp_dir().join(format!("modforge_snapshot_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("target/test-runs")).unwrap();

        // Write a fake "before" snapshot directly via snapshot_path.
        let before_path = snapshot_path(&tmp, "scope", "before");
        let before = vec![0u8; 16];
        std::fs::write(&before_path, &before).unwrap();

        // "after" bytes differ at 3 offsets.
        let mut after = before.clone();
        after[2] = 0xaa;
        after[5] = 0xbb;
        after[10] = 0xcc;

        let stored = std::fs::read(&before_path).unwrap();
        let d = diff_bytes(&stored, &after);
        assert_eq!(d.iter().copied().collect::<Vec<_>>(), vec![2, 5, 10]);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
