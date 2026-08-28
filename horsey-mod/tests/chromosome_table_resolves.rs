//! C1 verification: the CRISPR chromosome table resolves via
//! patternsleuth, validates structurally, and dumps as
//! (chromosome_id, slot, horse_offset, flat_idx) triples.
//!
//! Asserts:
//! - Resolver returns a non-null address.
//! - Address is i32-aligned.
//! - 240 valid (non--1) entries across 20 chromosomes of 17 slots.
//! - Every entry is a flat gene index in 0..240 and each value appears exactly once.

mod common;

use serde_json::{Value, json};

const STRIDE: usize = 0x44;
const SLOTS_PER_CHROMO: usize = 17;
const MAX_CHROMOSOMES: usize = 20;
const TABLE_BYTES: usize = STRIDE * MAX_CHROMOSOMES;

#[test]
fn chromosome_table_resolves() {
    let Some(game) = common::launch("chromosome_table_resolves") else {
        return;
    };

    let r = game
        .op_json("targets.resolve.chromosome_table", &json!({}))
        .unwrap();
    let addr_s = r
        .get("result")
        .and_then(|x| x.get("address"))
        .and_then(Value::as_str)
        .unwrap_or("0x0");
    let addr = u64::from_str_radix(addr_s.trim_start_matches("0x"), 16).unwrap_or(0);
    eprintln!("chromosome_table resolved to {addr_s}");
    assert!(addr != 0, "resolver returned null");
    assert_eq!(addr & 3, 0, "table not i32-aligned");

    let rb = game
        .op_json(
            "patterns.read_bytes",
            &json!({
                "addr": addr_s, "n": TABLE_BYTES,
            }),
        )
        .unwrap();
    let bytes_str = rb
        .get("result")
        .and_then(|x| x.get("bytes"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let bytes: Vec<u8> = bytes_str
        .split_whitespace()
        .filter_map(|h| u8::from_str_radix(h, 16).ok())
        .collect();
    assert_eq!(bytes.len(), TABLE_BYTES, "short read");

    let mut valid = 0usize;
    let mut flat_seen = vec![false; 240];
    let mut chromos_with_genes = 0usize;
    for cid in 0..MAX_CHROMOSOMES {
        let mut cells = Vec::new();
        let mut count = 0usize;
        for s in 0..SLOTS_PER_CHROMO {
            let off = cid * STRIDE + s * 4;
            let v =
                i32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]]);
            if v == -1 {
                break;
            }
            count += 1;
            valid += 1;
            assert!(
                (0..240).contains(&v),
                "chromo {cid} slot {s}: flat idx {v} out of range"
            );
            assert!(
                !flat_seen[v as usize],
                "chromo {cid} slot {s}: flat idx {v} duplicated"
            );
            flat_seen[v as usize] = true;
            cells.push(format!("{v:>3}"));
        }
        if count > 0 {
            chromos_with_genes += 1;
            eprintln!("chromo {cid:>2}: [{}]   ({count} genes)", cells.join(" "));
        }
    }

    eprintln!(
        "\nSUMMARY: {valid} valid entries across {chromos_with_genes}/{MAX_CHROMOSOMES} chromosomes"
    );
    assert_eq!(valid, 240, "expected exactly 240 valid entries");
    let unseen: Vec<usize> = (0..240).filter(|&i| !flat_seen[i]).collect();
    assert!(unseen.is_empty(), "missing flat indices: {unseen:?}");
    eprintln!("[PASS] table verified: 240 unique flat indices in 0..240");
}
