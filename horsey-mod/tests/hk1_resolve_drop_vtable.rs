//! HK1 research: resolve the Location vtable's drop-commit slot.
//!
//! From `FUN_1400d2ab0` (the click-drag handler) line 1722:
//!     cVar4 = (**(code **)(*param_1 + 0x78))(param_1);
//! That call returns 0 on drop-fail (game then plays DropHorseFail).
//! Returns non-zero on success. So vtable[+0x78] IS the drop-commit
//! method on the Location class.
//!
//! Home Location lives at scene-table slot 0x00 and has vtable RVA
//! 0x30f3d0 (confirmed via hk1.probe.scene_slot_vtables).
//!
//! This test:
//!   1. Reads *(image_base + 0x30f3d0) to confirm vtable address.
//!   2. Reads *(vtable + 0x78) to get the drop-commit function pointer.
//!   3. Reports the function's RVA so we can look up its decomp file.
//!
//! Designed for MODFORGE_ATTACH=1.

mod common;

use serde_json::{Value, json};
use std::time::Duration;

#[test]
fn resolve_drop_vtable() {
    let Some(game) = common::launch("hk1_resolve_drop_vtable") else {
        return;
    };
    common::wait_for_target_horse(&game, Duration::from_secs(60));

    let diag = game.op_json("gamestate.diag", &json!({})).expect("diag");
    let result = diag.get("result").unwrap_or(&diag);
    eprintln!(
        "diag result keys: {:?}",
        result.as_object().map(|m| m.keys().collect::<Vec<_>>())
    );

    // Get image_base via the active_location probe (it reports image_base).
    let probe = game
        .op_json("hk1.probe.active_location", &json!({}))
        .expect("probe");
    let probe_result = probe.get("result").unwrap_or(&probe);
    let image_base_s = probe_result
        .get("image_base")
        .and_then(Value::as_str)
        .expect("no image_base in probe");
    let image_base =
        u64::from_str_radix(image_base_s.trim_start_matches("0x"), 16).expect("bad image_base");
    eprintln!("image_base = {image_base:#x}");

    // Home Location vtable from S0 finding: RVA 0x30f3d0.
    let vtable_rva: u64 = 0x30f3d0;
    let vtable_addr = image_base + vtable_rva;
    eprintln!("home Location vtable @ {vtable_addr:#x} (RVA {vtable_rva:#x})");

    // Read several vtable slots so we can map all interesting methods
    // out of the click-handler decomp at once.
    let interesting_slots: &[u64] = &[0x38, 0x60, 0x68, 0x70, 0x78, 0x88, 0x90];
    let mut entries = Vec::new();
    for off in interesting_slots {
        let slot_addr = vtable_addr + off;
        let r = game
            .op_json("mem.peek", &json!({"addr": slot_addr, "kind": "u64"}))
            .unwrap();
        let val_s = r
            .get("result")
            .and_then(|x| x.get("value"))
            .and_then(Value::as_str)
            .unwrap_or("0x0");
        let fn_addr = u64::from_str_radix(val_s.trim_start_matches("0x"), 16).unwrap_or(0);
        let fn_rva = fn_addr.saturating_sub(image_base);
        entries.push((off, fn_addr, fn_rva));
    }

    eprintln!("\nvtable slots on Home Location:");
    eprintln!("  slot   fn addr             fn RVA");
    for (off, addr, rva) in &entries {
        let decomp_path = format!(
            "research/decompiled/funcs/{:05x?}/{:09x}_FUN_{:x}.c",
            (rva >> 16) & 0xff,
            rva,
            rva
        );
        eprintln!("  +0x{off:02x}   {addr:#018x}  {rva:#010x}    -> FUN_{rva:x}");
        eprintln!("            decomp: {decomp_path}");
    }

    game.pass("resolved vtable slots");
}
