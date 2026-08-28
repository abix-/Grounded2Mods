//! Generic: find every `.text` instruction whose disp32 references
//! a given target address. Wraps the `mem.find_xrefs` op with env-
//! var-driven inputs for ad-hoc investigation.
//!
//! Env (one of):
//!   MODFORGE_TARGET_RVA   PE-relative virtual address
//!   MODFORGE_TARGET_ADDR  absolute runtime address
//!
//! Example: find every load of GAMESTATE_PTR slot:
//!   MODFORGE_TARGET_RVA=0x1403fb0d8 \

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
fn find_xrefs_to_target() {
    let rva = need("MODFORGE_TARGET_RVA").map(|s| parse_hex_or_dec(&s));
    let addr = need("MODFORGE_TARGET_ADDR").map(|s| parse_hex_or_dec(&s));
    if rva.is_none() && addr.is_none() {
        eprintln!("skipping: set MODFORGE_TARGET_RVA or MODFORGE_TARGET_ADDR to use");
        return;
    }
    let Some(game) = common::launch("research_find_xrefs") else {
        return;
    };
    let target = if let Some(a) = addr {
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
        ib + (rva.unwrap() - 0x140000000)
    };
    let resp = game
        .op_json("mem.find_xrefs", &json!({"target_addr": target}))
        .expect("mem.find_xrefs must succeed");
    let result = resp.get("result").expect("result");
    let hits = result
        .get("hits")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    game.log()
        .event("XREFS", &format!("target=0x{target:x} hits={}", hits.len()));
    for h in &hits {
        game.log().event(
            "XREF",
            &format!(
                "instr@{} ({}): {}",
                h.get("instr_addr").and_then(|v| v.as_str()).unwrap_or("?"),
                h.get("opcode_class")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?"),
                h.get("context_hex").and_then(|v| v.as_str()).unwrap_or("?"),
            ),
        );
    }
    game.pass(&format!("found {} xrefs to 0x{target:x}", hits.len()));
}
