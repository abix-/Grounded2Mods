//! HK1: dump the .rdata around the trailer-rectangle constants and
//! interpret as both f32 and f64, to recover the real extent values
//! (the click handler FUN_1400d2ab0 ~714-728 tests position vs trailer
//! origin + these extents; the enter handler uses _DAT_14030eb80 as a
//! DOUBLE, so the region is mixed-width and a flat f32 read misaligns).

mod common;

use serde_json::{json, Value};

fn read_bytes(game: &modforge::harness::RunningGame, abs: u64, n: usize) -> Vec<u8> {
    let v = game
        .op_json("patterns.read_bytes", &json!({"addr": format!("0x{abs:x}"), "n": n}))
        .unwrap_or(Value::Null);
    let hex = v
        .get("result")
        .unwrap_or(&v)
        .get("bytes")
        .and_then(Value::as_str)
        .unwrap_or("");
    hex.split_whitespace().filter_map(|x| u8::from_str_radix(x, 16).ok()).collect()
}

fn dump(game: &modforge::harness::RunningGame, image_base: u64, label: &str, va: u64, n: usize) {
    let rva = va - 0x140000000;
    let abs = image_base + rva;
    let b = read_bytes(game, abs, n);
    eprintln!("[DUMP] {label} rva=0x{rva:x} ({n} bytes)");
    for off in (0..b.len().saturating_sub(3)).step_by(4) {
        let f = f32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]);
        let d = if off + 8 <= b.len() {
            let mut a = [0u8; 8];
            a.copy_from_slice(&b[off..off + 8]);
            Some(f64::from_le_bytes(a))
        } else {
            None
        };
        eprintln!("    +0x{:02x} (rva 0x{:x}): f32={f:<16} f64={d:?}", off, rva + off as u64);
    }
}

#[test]
fn dump_trailer_rect_consts() {
    let Some(game) = common::launch("hk1_trailer_rect") else { return };
    let info = game.op_json("game.build_info", &json!({})).expect("build_info");
    let ib = info.get("result").unwrap_or(&info).get("image_base").and_then(Value::as_str).unwrap_or("0x0");
    let image_base = u64::from_str_radix(ib.trim_start_matches("0x"), 16).unwrap_or(0);
    eprintln!("[DUMP] image_base={ib}");

    // x extents region (eb80 used as f64; eb8c/eb90 the click-handler x bounds)
    dump(&game, image_base, "x-region @ 0x14030eb80", 0x14030eb80, 32);
    // y-bottom DAT_140303374 (read clean as 1.25 earlier)
    dump(&game, image_base, "y-lo @ 0x140303368", 0x140303368, 24);
    // y-top DAT_14030d9b8
    dump(&game, image_base, "y-hi @ 0x14030d9b0", 0x14030d9b0, 24);
}
