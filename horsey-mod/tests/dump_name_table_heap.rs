//! Scan heap for a string, parse each hit via
//! [`modforge::testkit::msvc::MsvcStdString`], assert at least one
//! SSO header matches the needle.

mod common;

use modforge::testkit::msvc::MsvcStdString;
use serde_json::{Value, json};

#[test]
fn dump_name_table_heap() {
    let Some(game) = common::launch("dump_name_table_heap") else {
        return;
    };
    let needle = std::env::var("HORSEY_HEAP_NEEDLE").unwrap_or_else(|_| "tomtato".into());
    let max_hits: u32 = common::env_or("HORSEY_HEAP_MAX_HITS", 20u32);
    let stride: usize = std::env::var("HORSEY_HEAP_STRIDE")
        .ok()
        .and_then(|s| {
            let t = s.trim().trim_start_matches("0x").trim_start_matches("0X");
            usize::from_str_radix(t, 16).ok()
        })
        .unwrap_or(0x88);

    let s = game
        .op_json(
            "mem.scan_heap_string",
            &json!({
                "needle": &needle, "max_hits": max_hits,
            }),
        )
        .unwrap();
    let hits = s
        .get("result")
        .and_then(|r| r.get("hits"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    eprintln!("scan for '{needle}' returned {} hit(s)", hits.len());
    assert!(!hits.is_empty(), "needle '{needle}' not found on heap");

    let mut sso_match_count = 0;
    for h in &hits {
        let addr_s = h.get("addr").and_then(Value::as_str).unwrap_or("0x0");
        let r = game
            .op_json(
                "patterns.read_bytes",
                &json!({"addr": addr_s, "n": 0x100u64}),
            )
            .unwrap();
        let hex = r
            .get("result")
            .and_then(|rr| rr.get("bytes"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let bytes: Vec<u8> = hex
            .split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect();
        if bytes.len() < 0x20 {
            continue;
        }
        let s = MsvcStdString::parse(&bytes).expect("std::string parse");
        eprintln!(
            "hit @ {addr_s}: inline=[{}] size={} cap={}",
            s.ascii_inline(),
            s.size,
            s.capacity
        );
        if s.ascii_inline().starts_with(&needle) {
            sso_match_count += 1;
        }
        if bytes.len() >= stride + 0x20 {
            if let Some(next) = MsvcStdString::parse(&bytes[stride..]) {
                eprintln!(
                    "  +0x{stride:x}: inline=[{}] size={} cap={}",
                    next.ascii_inline(),
                    next.size,
                    next.capacity
                );
            }
        }
    }

    assert!(
        sso_match_count > 0,
        "no hit's inline SSO matched '{needle}'"
    );
    if let Ok(want) = std::env::var("HORSEY_EXPECT_HITS") {
        let want: usize = want.parse().expect("HORSEY_EXPECT_HITS not a number");
        assert_eq!(hits.len(), want);
    }
    game.pass(&format!(
        "'{needle}': {} hits, {sso_match_count} sso-confirmed",
        hits.len()
    ));
}
