//! Thin env-driven wrapper around `modforge::research::read_uint`
//! / `read_int`. Decode an integer operand at a known address.
//!
//! Env:
//!   MODFORGE_ADDR    absolute runtime addr   (or)
//!   MODFORGE_RVA     PE-relative virtual address
//!   MODFORGE_OFFSET  byte offset inside the instruction (default 0)
//!   MODFORGE_SIZE    1 | 2 | 4 | 8 (default 4)
//!   MODFORGE_SIGNED  1 to also report signed decoding (default 0)

mod common;

use serde_json::json;

fn need(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}
fn parse_hex_or_dec(s: &str) -> u64 {
    if let Some(stripped) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(stripped, 16).unwrap_or(0)
    } else {
        s.parse().unwrap_or(0)
    }
}

#[test]
fn decode_operand_at_address() {
    let addr_env = need("MODFORGE_ADDR").map(|s| parse_hex_or_dec(&s));
    let rva_env = need("MODFORGE_RVA").map(|s| parse_hex_or_dec(&s));
    if addr_env.is_none() && rva_env.is_none() {
        eprintln!("skipping: set MODFORGE_ADDR or MODFORGE_RVA");
        return;
    }
    let offset = need("MODFORGE_OFFSET")
        .map(|s| parse_hex_or_dec(&s) as usize)
        .unwrap_or(0);
    let size = need("MODFORGE_SIZE")
        .map(|s| parse_hex_or_dec(&s) as usize)
        .unwrap_or(4);
    let want_signed = need("MODFORGE_SIGNED")
        .map(|s| s == "1" || s == "true")
        .unwrap_or(false);

    let Some(game) = common::launch("research_decode_operand") else {
        return;
    };
    let addr = if let Some(a) = addr_env {
        a
    } else {
        let r = game
            .op_json("targets.resolve.gamestate_ptr", &json!({}))
            .expect("ok");
        let ib = u64::from_str_radix(
            r.get("result")
                .unwrap()
                .get("image_base")
                .unwrap()
                .as_str()
                .unwrap()
                .trim_start_matches("0x"),
            16,
        )
        .unwrap();
        ib + (rva_env.unwrap() - 0x140000000)
    };
    let read_addr = addr.wrapping_add(offset as u64);
    let u = modforge::research::read_uint(&game, read_addr, size).expect("read_uint");
    let mut report = format!("addr=0x{read_addr:x} size={size} unsigned=0x{u:x} ({u})");
    if want_signed {
        let s = modforge::research::read_int(&game, read_addr, size).expect("read_int");
        report.push_str(&format!(" signed={s} ({s:#x})"));
    }
    game.log().event("DECODE", &report);
    game.pass(&report);
}
