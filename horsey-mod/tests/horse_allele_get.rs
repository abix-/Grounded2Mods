//! GENERIC cmdlet: read ANY allele(s) on ANY horse.
//!
//! Single 0..=479 address space (see horse_allele_set.rs).
//!
//! Env vars:
//! - `HORSEY_HORSE=<name>` -> target horse (default: first owned).
//! - `HORSEY_IDX=<0..=479>` -> read just this allele; otherwise dump
//!                             all 480 as a vanilla block + ext block.

mod common;

use serde_json::{Value, json};

const VANILLA_LEN: usize = 240;
const EXT_LEN: usize = 240;
const TOTAL: usize = VANILLA_LEN + EXT_LEN;

#[test]
fn horse_allele_get() {
    let Some(game) = common::launch("horse_allele_get") else {
        return;
    };
    let h = common::target_horse(&game);
    eprintln!("target: {} (id {})", h.name, h.id);

    if let Ok(idx_s) = std::env::var("HORSEY_IDX") {
        let idx: usize = idx_s.parse().expect("HORSEY_IDX not a number");
        assert!(idx < TOTAL, "HORSEY_IDX {idx} out of range 0..{TOTAL}");
        if idx < VANILLA_LEN {
            let r = game
                .op_json(
                    "horse.vanilla.alleles.get",
                    &json!({
                        "addr": h.ptr_s, "idx": idx,
                    }),
                )
                .unwrap();
            let v = r
                .get("result")
                .and_then(|x| x.get("value"))
                .and_then(Value::as_u64);
            eprintln!("vanilla[{idx}] = {v:?}");
        } else {
            let ext_idx = (idx - VANILLA_LEN) as u32;
            let r = game
                .op_json(
                    "horse.ext.alleles.get",
                    &json!({
                        "horse_id": h.id, "ext_gene_idx": ext_idx,
                    }),
                )
                .unwrap();
            eprintln!(
                "ext[{ext_idx}] (idx {idx}) = {}",
                r.get("result").unwrap_or(&Value::Null)
            );
        }
        return;
    }

    // Full dump: vanilla 240 first, then ext genome.
    let vr = game
        .op_json("horse.vanilla.genome.get", &json!({"addr": h.ptr_s}))
        .unwrap();
    let varr = vr
        .get("result")
        .and_then(|x| x.get("alleles"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let vbytes: Vec<u8> = varr
        .iter()
        .filter_map(|v| v.as_u64().map(|n| n as u8))
        .collect();
    let vnz = vbytes.iter().filter(|&&b| b != 0).count();
    eprintln!("\n=== VANILLA ({} bytes, {vnz} nonzero) ===", vbytes.len());
    for row in 0..15 {
        eprint!("  ");
        for col in 0..16 {
            eprint!("{:>3} ", vbytes[row * 16 + col]);
        }
        eprintln!();
    }

    let er = game
        .op_json("horse.ext.genome.get", &json!({"horse_id": h.id}))
        .unwrap();
    let earr = er
        .get("result")
        .and_then(|x| x.get("alleles"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let ebytes: Vec<u8> = earr
        .iter()
        .filter_map(|v| v.as_u64().map(|n| n as u8))
        .collect();
    let enz = ebytes.iter().filter(|&&b| b != 0).count();
    eprintln!("\n=== EXT ({} bytes, {enz} nonzero) ===", ebytes.len());
    if !ebytes.is_empty() {
        let rows = ebytes.len().div_ceil(16);
        for row in 0..rows {
            eprint!("  ");
            for col in 0..16 {
                let i = row * 16 + col;
                if i >= ebytes.len() {
                    break;
                }
                eprint!("{:>3} ", ebytes[i]);
            }
            eprintln!();
        }
    }
}
