//! Thin wrapper over [`modforge::testkit::fn_entry::verify_fn_entry`].
//! Default RVA is APPLY_GENE_TO_HORSE for backward compat.

mod common;

use modforge::testkit::fn_entry;

#[test]
fn verify_fn_entry_bytes() {
    let Some(game) = common::launch("verify_fn_entry_bytes") else {
        return;
    };
    // Default RVA matches the pre-extraction behavior so a bare
    // `cargo test` keeps working.
    let mut cfg = match fn_entry::Config::from_env("HORSEY_FN") {
        Ok(c) => c,
        Err(_) => {
            // No HORSEY_FN_RVA set: default to APPLY_GENE_TO_HORSE so a
            // bare `cargo test` keeps working.
            let mut c = fn_entry::Config::defaults();
            c.rva = 0x9f670;
            c
        }
    };
    let _ = &mut cfg;
    let v = fn_entry::verify_fn_entry(&game, &cfg).unwrap_or_else(|e| panic!("{e}"));
    game.pass(&format!(
        "function entry at rva 0x{:x} verified (prologue_ok={})",
        cfg.rva, v.prologue_ok
    ));
}
