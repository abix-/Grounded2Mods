//! Dump the first 128 bytes of `FUN_1400de2e0` (Home Location's
//! drop-commit, vtable[+0x78]). The SEH guard confirmed it AVs at
//! `entry + 0x31` consistently; we want the instruction bytes there
//! so we can decode which register/field it dereffs.

mod common;

use serde_json::{Value, json};

#[test]
fn dump_drop_commit_bytes() {
    let Some(game) = common::launch("hk1_dump_drop_commit_bytes") else {
        return;
    };

    // Read image base from the active_location probe (it already
    // surfaces image_base and the resolved Home Location vtable).
    let probe = game
        .op_json("hk1.probe.active_location", &json!({}))
        .expect("probe");
    let result = probe.get("result").unwrap_or(&probe);
    let image_base_s = result
        .get("image_base")
        .and_then(Value::as_str)
        .expect("no image_base");
    let image_base = u64::from_str_radix(image_base_s.trim_start_matches("0x"), 16).expect("bad");
    eprintln!("image_base = {image_base:#x}");

    // FUN_1400de2e0 = drop-commit. RVA 0xde2e0.
    let target_rva: u64 = 0xde2e0;
    let target_addr = image_base + target_rva;
    eprintln!("dumping FUN_1400de2e0 @ {target_addr:#x} (RVA {target_rva:#x})");

    let r = game
        .op_json(
            "patterns.read_bytes",
            &json!({
                "addr": format!("0x{target_addr:x}"),
                "n": 128u64,
            }),
        )
        .expect("read_bytes");
    let hex = r
        .get("result")
        .and_then(|x| x.get("bytes"))
        .and_then(Value::as_str)
        .expect("no bytes")
        .to_string();
    let bytes: Vec<u8> = hex
        .split_whitespace()
        .map(|s| u8::from_str_radix(s, 16).unwrap_or(0))
        .collect();

    eprintln!("\nbytes (16 per row, marker ! on the fault offset +0x31):");
    for (row, chunk) in bytes.chunks(16).enumerate() {
        let off = row * 16;
        let cells: String = chunk
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let abs = off + i;
                if abs == 0x31 {
                    format!("!{b:02x}")
                } else {
                    format!(" {b:02x}")
                }
            })
            .collect::<Vec<_>>()
            .join("");
        eprintln!("  +0x{off:03x} {cells}");
    }
    eprintln!("\nfault is at +0x31. Read backward from there to find the instruction.");
    game.pass(&format!("dumped {} bytes", bytes.len()));
}
