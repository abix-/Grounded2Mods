//! Thin env-driven wrapper around
//! `modforge::research::decode_imm_at_call_site`. For every
//! `e8 X<target_fn>` call, decode the imm passed in via the
//! preceding `mov reg, imm32` (or similar). Histogram the values.
//!
//! Env:
//!   MODFORGE_TARGET_FN   (required) absolute addr or RVA of the callee
//!   MODFORGE_IMM_OPCODE  (required) e.g. "b9" for mov ecx imm32,
//!                                   "ba" for mov edx imm32, "68" for push imm32
//!   MODFORGE_IMM_OFF     (optional, default 1) offset of imm inside match
//!   MODFORGE_IMM_SIZE    (optional, default 4) 1 or 4
//!   MODFORGE_LOOKBACK    (optional, default 16) bytes before each call

mod common;

use serde_json::json;

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

#[test]
fn decode_imm_at_call_site() {
    let Some(target_raw) = need("MODFORGE_TARGET_FN").map(|s| parse_uint(&s)) else {
        eprintln!("skipping: set MODFORGE_TARGET_FN + MODFORGE_IMM_OPCODE");
        return;
    };
    let imm_opcode = need("MODFORGE_IMM_OPCODE").expect("MODFORGE_IMM_OPCODE required");
    let imm_off = need("MODFORGE_IMM_OFF")
        .map(|s| parse_uint(&s) as usize)
        .unwrap_or(1);
    let imm_size = need("MODFORGE_IMM_SIZE")
        .map(|s| parse_uint(&s) as usize)
        .unwrap_or(4);
    let lookback = need("MODFORGE_LOOKBACK")
        .map(|s| parse_uint(&s) as u32)
        .unwrap_or(16);

    let Some(game) = common::launch("research_decode_imm_at_call_site") else {
        return;
    };
    let target = if target_raw < 0x200000000 {
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
        ib + (target_raw - 0x140000000)
    } else {
        target_raw
    };
    let hist = modforge::research::decode_imm_at_call_site(
        &game,
        target,
        &imm_opcode,
        imm_off,
        imm_size,
        lookback,
    )
    .expect("decode_imm_at_call_site");

    let mut top: Vec<(i64, usize)> = hist.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    for (i, (val, count)) in top.iter().take(10).enumerate() {
        game.log().event(
            "HISTOGRAM",
            &format!("[{i}] imm={val:#x} ({val}) count={count}"),
        );
    }
    game.pass(&format!(
        "scanned call sites; {} distinct imm values",
        top.len()
    ));
}
