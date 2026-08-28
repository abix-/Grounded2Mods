//! GENERIC cmdlet: set ANY allele on ANY horse.
//!
//! Single address space: idx 0..=479. idx < 240 -> vanilla allele
//! (single u8). idx >= 240 -> extended allele (ext_gene_idx =
//! idx - 240, maternal=paternal=value clamped to 0..=3).
//!
//! Env vars:
//! - `HORSEY_HORSE=<name>` -> target horse (default: first owned).
//! - `HORSEY_IDX=<0..=479>` -> allele index (default: 0).
//! - `HORSEY_VALUE=<u8>` -> value to write (default: 0).
//!                          For ext alleles, clamped to 0..=3.
//! - `HORSEY_ALL=1` -> write `HORSEY_VALUE` to ALL 480 alleles.
//! - `HORSEY_ARM_RENDER=1` -> arm D5 render trampoline before write
//!                            (required for ext writes to be visible).
//!
//! Examples:
//!   HORSEY_VALUE=3 HORSEY_ALL=1 cargo test --test horse_allele_set
//!     -> max-out every allele (vanilla + ext) on first-owned horse.
//!   HORSEY_HORSE=tomtato HORSEY_IDX=240 HORSEY_VALUE=3 HORSEY_ARM_RENDER=1
//!     -> tomtato ext[0] = giant.

mod common;

use serde_json::{Value, json};

const VANILLA_LEN: usize = 240;
const EXT_LEN: usize = 240;
const TOTAL: usize = VANILLA_LEN + EXT_LEN;

#[test]
fn horse_allele_set() {
    let Some(game) = common::launch("horse_allele_set") else {
        return;
    };
    let h = common::target_horse(&game);
    eprintln!("target: {} (id {})", h.name, h.id);

    let value: u8 = common::env_or("HORSEY_VALUE", 0u32) as u8;
    let all = std::env::var("HORSEY_ALL").ok().as_deref() == Some("1");

    if std::env::var("HORSEY_ARM_RENDER").ok().as_deref() == Some("1") {
        let r = game.op_json("genes.ext.render.arm", &json!({})).unwrap();
        eprintln!("render.arm: {r}");
    }

    if all {
        // Vanilla: 240 bytes of `value`.
        let alleles: Vec<u8> = vec![value; VANILLA_LEN];
        let r = game
            .op_json(
                "horse.vanilla.genome.set",
                &json!({
                    "addr": h.ptr_s, "alleles": alleles,
                }),
            )
            .unwrap();
        assert_eq!(
            r.get("ok").and_then(Value::as_bool),
            Some(true),
            "vanilla genome.set: {r}"
        );
        // Ext: 240 slots of (value,value) clamped 0..=3.
        let v3 = value.min(3);
        for ext_idx in 0..EXT_LEN as u32 {
            let r = game
                .op_json(
                    "horse.ext.alleles.set",
                    &json!({
                        "horse_id": h.id, "ext_gene_idx": ext_idx,
                        "maternal": v3, "paternal": v3,
                    }),
                )
                .unwrap();
            assert_eq!(
                r.get("ok").and_then(Value::as_bool),
                Some(true),
                "ext.set ext_idx={ext_idx}: {r}"
            );
        }
        eprintln!(
            "[PASS] all {TOTAL} alleles on '{}' set to {value} (ext clamped to {v3})",
            h.name
        );
        return;
    }

    let idx: usize = common::env_or("HORSEY_IDX", 0usize);
    assert!(idx < TOTAL, "HORSEY_IDX {idx} out of range 0..{TOTAL}");

    if idx < VANILLA_LEN {
        let r = game
            .op_json(
                "horse.vanilla.alleles.set",
                &json!({
                    "addr": h.ptr_s, "idx": idx, "value": value as u64,
                }),
            )
            .unwrap();
        assert_eq!(
            r.get("ok").and_then(Value::as_bool),
            Some(true),
            "vanilla.set: {r}"
        );
        eprintln!("[PASS] '{}' vanilla[{idx}] = {value}", h.name);
    } else {
        let ext_idx = (idx - VANILLA_LEN) as u32;
        let v3 = value.min(3);
        let r = game
            .op_json(
                "horse.ext.alleles.set",
                &json!({
                    "horse_id": h.id, "ext_gene_idx": ext_idx,
                    "maternal": v3, "paternal": v3,
                }),
            )
            .unwrap();
        assert_eq!(
            r.get("ok").and_then(Value::as_bool),
            Some(true),
            "ext.set: {r}"
        );
        eprintln!(
            "[PASS] '{}' ext[{ext_idx}] = ({v3},{v3}) (idx {idx}, clamped from {value})",
            h.name
        );
    }
}
