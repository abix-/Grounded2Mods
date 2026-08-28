//! End-to-end proof that the shipped bestiary file deploys with the
//! mod and the injected DLL auto-loads it.
//!
//! Flow:
//!   1. Harness builds + launches the game + injects horsey.dll.
//!      As part of staging, the injector copies
//!      `horsey-mod/bestiary/genes-extended.xml` next to the staged
//!      DLL (skipping if a user copy already exists there).
//!   2. `worker_main` auto-loads the file via `genes_xml::load_from_file`.
//!   3. We query `genes.ext.get { ext_idx: 0 }` and assert it returns
//!      `BX_GIANT_BABY` with `render: { slot: 0, mode: "set" }` and
//!      `alleles: [0, 50, 100, 200]`.
//!
//! Catches deployment / loader regressions that the unit-level
//! `bestiary_v1.rs` contract cannot see.

mod common;

use serde_json::json;

#[test]
fn bestiary_v1_lands_after_inject() {
    let Some(game) = common::launch("bestiary_v1_live") else {
        return;
    };

    let resp = game
        .op_json("genes.ext.get", &json!({"ext_idx": 0}))
        .expect("genes.ext.get must succeed");
    let result = resp.get("result").expect("response must have a result");

    let name = result
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert_eq!(
        name, "BX_GIANT_BABY",
        "ext idx 0 is not BX_GIANT_BABY: {resp}"
    );

    let alleles = result
        .get("alleles")
        .and_then(|v| v.as_array())
        .expect("alleles array");
    let alleles_u: Vec<u64> = alleles.iter().filter_map(|a| a.as_u64()).collect();
    assert_eq!(
        alleles_u,
        vec![0u64, 50, 100, 200],
        "g0..g3 ladder mismatch: {resp}"
    );

    let render = result
        .get("render")
        .expect("render mapping must be present");
    let slot = render
        .get("slot")
        .and_then(|v| v.as_u64())
        .unwrap_or(u64::MAX);
    let mode = render
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert_eq!(slot, 0, "render.slot must target the verified slot");
    assert_eq!(mode, "set", "render.mode must be 'set'");

    game.pass("bestiary v1 deployed + auto-loaded + visible via HTTP");
}
