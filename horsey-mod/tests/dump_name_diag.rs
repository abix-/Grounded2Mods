//! GENERIC: invoke `horse.name_diag` for each name id in a list and
//! assert each resolves to a readable name entry.
//!
//! Modes (env-driven):
//! - `HORSEY_NAME_IDS=<csv>` (default `250,251,272`): comma-separated
//!   name ids (decimal or `0x` hex). Each is queried in turn.
//! - default: assert each id returns a result with `entry_readable=true`.
//! - `HORSEY_EXPECT_READABLE=0`: relax assertion (just print and pass).
//!   Use to survey id ranges where some are expected to miss.

mod common;

use serde_json::{Value, json};

#[test]
fn dump_name_diag() {
    let Some(game) = common::launch("dump_name_diag") else {
        return;
    };
    let ids_csv = std::env::var("HORSEY_NAME_IDS").unwrap_or_else(|_| "250,251,272".into());
    let strict = common::env_or("HORSEY_EXPECT_READABLE", 1u8) != 0;

    let ids: Vec<u32> = ids_csv
        .split(',')
        .map(|s| {
            let t = s.trim();
            if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                u32::from_str_radix(h, 16).unwrap_or_else(|e| panic!("name id '{t}': {e}"))
            } else {
                t.parse().unwrap_or_else(|e| panic!("name id '{t}': {e}"))
            }
        })
        .collect();
    assert!(!ids.is_empty(), "HORSEY_NAME_IDS yielded no ids");

    let mut readable_count = 0;
    for nid in &ids {
        let r = game
            .op_json("horse.name_diag", &json!({"name_id": nid}))
            .unwrap();
        let result = r.get("result").unwrap_or(&r);
        let readable = result
            .get("entry_readable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        eprintln!("\nname_id={nid}: readable={readable} {result}");
        if readable {
            readable_count += 1;
        }
    }

    if strict {
        assert_eq!(
            readable_count,
            ids.len(),
            "HORSEY_EXPECT_READABLE=1: only {readable_count}/{} ids had a readable entry",
            ids.len()
        );
    }

    game.pass(&format!(
        "{}/{} name ids resolved",
        readable_count,
        ids.len()
    ));
}
