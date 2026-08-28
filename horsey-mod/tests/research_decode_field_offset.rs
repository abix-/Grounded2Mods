//! Thin env-driven wrapper around
//! `modforge::research::decode_field_offset_via_string`. Locate a
//! struct-field offset by anchoring on a `.rdata` string literal
//! referenced by the function that accesses the field.
//!
//! Env:
//!   MODFORGE_STRING_HEX  (required) hex bytes of the .rdata anchor literal
//!   MODFORGE_LEA_OFFSET  (optional, default 0; signed)
//!   MODFORGE_DISP_OPCODE (required) e.g. "8b 81" for mov r32, [rcx+disp32]
//!   MODFORGE_DISP_OFF    (required) byte offset of disp inside the matched opcode
//!   MODFORGE_DISP_SIZE   (optional, default 4) 1 or 4
//!   MODFORGE_WINDOW      (optional, default 64) bytes around each xref

mod common;

fn need(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}
fn parse_uint(s: &str) -> u64 {
    if let Some(stripped) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(stripped, 16).unwrap_or(0)
    } else {
        s.parse().unwrap_or(0)
    }
}
fn parse_signed(s: &str) -> i64 {
    let neg = s.starts_with('-');
    let body = if neg { &s[1..] } else { s };
    let m = parse_uint(body) as i64;
    if neg { -m } else { m }
}

#[test]
fn decode_field_offset_via_string_anchor() {
    let Some(string_hex) = need("MODFORGE_STRING_HEX") else {
        eprintln!("skipping: set MODFORGE_STRING_HEX + MODFORGE_DISP_OPCODE + MODFORGE_DISP_OFF");
        return;
    };
    let lea_offset = need("MODFORGE_LEA_OFFSET")
        .map(|s| parse_signed(&s))
        .unwrap_or(0);
    let disp_opcode = need("MODFORGE_DISP_OPCODE").expect("MODFORGE_DISP_OPCODE required");
    let disp_off = need("MODFORGE_DISP_OFF")
        .map(|s| parse_uint(&s) as usize)
        .expect("MODFORGE_DISP_OFF required");
    let disp_size = need("MODFORGE_DISP_SIZE")
        .map(|s| parse_uint(&s) as usize)
        .unwrap_or(4);
    let window: u64 = need("MODFORGE_WINDOW")
        .map(|s| parse_uint(&s))
        .unwrap_or(64);

    let Some(game) = common::launch("research_decode_field_offset") else {
        return;
    };
    let hist = modforge::research::decode_field_offset_via_string(
        &game,
        &string_hex,
        lea_offset,
        &disp_opcode,
        disp_off,
        disp_size,
        window,
    )
    .expect("decode_field_offset_via_string");

    let mut top: Vec<(i64, usize)> = hist.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    for (i, (disp, count)) in top.iter().take(5).enumerate() {
        game.log().event(
            "HISTOGRAM",
            &format!("[{i}] disp={disp:#x} ({disp}) count={count}"),
        );
    }
    if let Some((disp, count)) = top.first() {
        game.pass(&format!(
            "top field offset: {disp:#x} ({disp}) with {count} matches"
        ));
    } else {
        panic!("no disp matches found in any xref window");
    }
}
