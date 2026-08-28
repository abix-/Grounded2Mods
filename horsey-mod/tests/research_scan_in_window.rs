//! Thin env-driven wrapper around `modforge::research::scan_in_window`.

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
fn scan_within_window() {
    let Some(sig) = need("MODFORGE_SIG") else {
        eprintln!("skipping: set MODFORGE_SIG + MODFORGE_WINDOW_START + MODFORGE_WINDOW_LEN");
        return;
    };
    let start_raw = need("MODFORGE_WINDOW_START")
        .map(|s| parse_uint(&s))
        .expect("MODFORGE_WINDOW_START required");
    let win_len = need("MODFORGE_WINDOW_LEN")
        .map(|s| parse_uint(&s))
        .expect("MODFORGE_WINDOW_LEN required");
    let disp_off = need("MODFORGE_DISP32_OFF")
        .map(|s| parse_uint(&s) as u32)
        .unwrap_or(2);
    let sig_byte_count = sig.split_whitespace().count() as u32;
    let instr_len = need("MODFORGE_INSTR_LEN")
        .map(|s| parse_uint(&s) as u32)
        .unwrap_or(sig_byte_count);
    let max_hits = need("MODFORGE_MAX_HITS")
        .map(|s| parse_uint(&s) as u32)
        .unwrap_or(256);

    let Some(game) = common::launch("research_scan_in_window") else {
        return;
    };
    let start = if start_raw < 0x200000000 {
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
        ib + (start_raw - 0x140000000)
    } else {
        start_raw
    };
    let hits = modforge::research::scan_in_window(
        &game, &sig, disp_off, instr_len, start, win_len, max_hits,
    )
    .expect("scan_in_window");
    game.log().event(
        "WINDOW",
        &format!(
            "start=0x{start:x} end=0x{:x} hits={}",
            start + win_len,
            hits.len()
        ),
    );
    for h in &hits {
        game.log().event(
            "IN_WINDOW",
            &format!(
                "instr@0x{:x} target=0x{:x} disp={:#x}",
                h.instr_addr, h.decoded_target, h.disp32
            ),
        );
    }
    game.pass(&format!("{} hits in window", hits.len()));
}
