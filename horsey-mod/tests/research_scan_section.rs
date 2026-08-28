//! Generic: scan a PE section (`.text`, `.data`, or `.rdata`) for a
//! byte pattern and dump every hit address. Use to find string
//! literals (.rdata), vtables (.rdata), or runtime values (.data).
//!
//! Env:
//!   MODFORGE_SIG       (required) IDA-style sig with `??` wildcards
//!   MODFORGE_SECTION   (required) "text" | "data" | "rdata"
//!   MODFORGE_MAX_HITS  (optional, default 256)
//!
//! Example: locate the "retired %s (useless)" format string in .rdata:
//!   MODFORGE_SIG="72 65 74 69 72 65 64 20 25 73 20 28 75 73 65 6c 65 73 73 29" \
//!   MODFORGE_SECTION=rdata \

mod common;

use serde_json::json;

fn need(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

#[test]
fn scan_section_for_bytes() {
    let Some(sig) = need("MODFORGE_SIG") else {
        eprintln!("skipping: set MODFORGE_SIG + MODFORGE_SECTION to use");
        return;
    };
    let section = need("MODFORGE_SECTION").unwrap_or_else(|| "text".to_string());
    let max_hits = need("MODFORGE_MAX_HITS")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(256);

    let Some(game) = common::launch("research_scan_section") else {
        return;
    };

    let (op, args) = match section.as_str() {
        "rdata" => ("mem.scan_rdata", json!({"sig": sig, "max_hits": max_hits})),
        "data" => (
            "mem.scan_data",
            json!({"kind": "bytes", "bytes": sig, "max_hits": max_hits}),
        ),
        "text" => (
            "patterns.sleuth.scan_all",
            json!({
                "sig": sig, "disp32_offset": 0u32, "instr_len": 4u32, "context_bytes": 16u32, "max_hits": max_hits,
            }),
        ),
        other => panic!("MODFORGE_SECTION must be text|data|rdata, got {other:?}"),
    };

    let resp = game.op_json(op, &args).expect("op must succeed");
    let result = resp.get("result").expect("result");
    let hits = result
        .get("hits")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    game.log().event(
        "SCAN",
        &format!("section={section} sig={sig:?} hits={}", hits.len()),
    );
    for h in &hits {
        game.log().event(
            "HIT",
            &format!(
                "addr={}",
                h.get("addr")
                    .or_else(|| h.get("instr_addr"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?"),
            ),
        );
    }
    game.pass(&format!("section={section} {} hits", hits.len()));
}
