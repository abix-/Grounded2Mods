//! Generic: dump N raw bytes from the loaded image at a given RVA
//! or absolute runtime address.
//!
//! Drives signature authoring: read 32-64 bytes at a known function
//! entry or `.data` location, paste the hex into a sleuth sig.
//!
//! Env (one of):
//!   MODFORGE_RVA   PE-relative virtual address (e.g. 0x14009f670)
//!   MODFORGE_ADDR  absolute runtime address (e.g. 0x7ff7517a5b4f)
//! Optional:
//!   MODFORGE_N     number of bytes to read (default 32; max 4096)
//!
//! Example:
//!   MODFORGE_RVA=0x1400fd580 MODFORGE_N=48 \

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
fn dump_bytes_at_address() {
    let rva = need("MODFORGE_RVA").map(|s| parse_hex_or_dec(&s));
    let addr = need("MODFORGE_ADDR").map(|s| parse_hex_or_dec(&s));
    if rva.is_none() && addr.is_none() {
        eprintln!("skipping: set MODFORGE_RVA or MODFORGE_ADDR to use");
        return;
    }
    let n = need("MODFORGE_N")
        .map(|s| parse_hex_or_dec(&s))
        .unwrap_or(32);

    let Some(game) = common::launch("research_dump_bytes") else {
        return;
    };
    let target = if let Some(a) = addr {
        a
    } else {
        let r = game
            .op_json("targets.resolve.gamestate_ptr", &json!({}))
            .expect("ok");
        let ib_str = r
            .get("result")
            .unwrap()
            .get("image_base")
            .unwrap()
            .as_str()
            .unwrap();
        let ib = u64::from_str_radix(ib_str.trim_start_matches("0x"), 16).unwrap_or(0);
        ib + (rva.unwrap() - 0x140000000)
    };
    let resp = game
        .op_json(
            "patterns.read_bytes",
            &json!({"addr": format!("0x{target:x}"), "n": n}),
        )
        .expect("patterns.read_bytes must succeed");
    let bytes = resp
        .get("result")
        .and_then(|r| r.get("bytes"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    game.log().event(
        "DUMP",
        &format!(
            "rva={} addr=0x{target:x} n={n} bytes={bytes}",
            rva.map(|r| format!("0x{r:x}"))
                .unwrap_or_else(|| "?".to_string()),
        ),
    );
    game.pass(&format!("dumped {n} bytes at 0x{target:x}"));
}
