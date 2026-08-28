//! Thin wrapper over [`modforge::testkit::snapshot`]. Takes a horse-byte
//! snapshot, optionally diffs vs a saved tag.
//!
//! Env:
//!   HORSEY_HORSE=<name>     target horse (default: first owned)
//!   HORSEY_DUMP_TAG=<label> snapshot label (default `snapshot`)
//!   HORSEY_DUMP_DIFF=<tag>  if set, diff vs snapshot file with this tag
//!   HORSEY_EXPECT_DIFF_OFFSETS=<csv hex>

mod common;

use modforge::testkit::snapshot;

const HORSE_ALLOC_SIZE: usize = 0x498;

#[test]
fn dump_horse_bytes() {
    let Some(game) = common::launch("hk1_dump_horse_bytes") else {
        return;
    };
    let h = common::target_horse(&game);
    let tag = std::env::var("HORSEY_DUMP_TAG").unwrap_or_else(|_| "snapshot".into());
    eprintln!(
        "\n[{tag}] horse '{}' at {} (raw {:#x})",
        h.name, h.ptr_s, h.id
    );

    let cfg = snapshot::Config {
        addr: h.id,
        len: HORSE_ALLOC_SIZE,
        scope: format!("horse_dump_{}", h.name),
        tag: tag.clone(),
        workspace_root: snapshot::Config::workspace_from_manifest(env!("CARGO_MANIFEST_DIR")),
    };

    if let Ok(other_tag) = std::env::var("HORSEY_DUMP_DIFF") {
        let expected = std::env::var("HORSEY_EXPECT_DIFF_OFFSETS")
            .ok()
            .map(|s| snapshot::parse_offset_csv(&s).expect("EXPECT_DIFF_OFFSETS parse"));
        let diff = snapshot::diff_against(&game, &cfg, &other_tag, expected.as_ref())
            .unwrap_or_else(|e| panic!("{e}"));
        game.pass(&format!(
            "diff [{}] vs [{other_tag}]: {} byte(s) changed",
            tag,
            diff.changed_offsets.len()
        ));
    } else {
        let snap = snapshot::take(&game, &cfg).unwrap_or_else(|e| panic!("{e}"));
        game.pass(&format!(
            "snapshot [{tag}] {} bytes -> {}",
            snap.bytes.len(),
            snap.path.display()
        ));
    }
}
