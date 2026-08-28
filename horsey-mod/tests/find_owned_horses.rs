//! Validate the owned-horse chain GS+0x438 -> +0x90 -> +0x130/+0x138.
//!
//! Auto-launches the game via modforge::harness. The harness assumes the
//! configured Steam save loads automatically (vanilla Horsey behavior:
//! last-played save is restored on launch).
//!
//! The investigation test before this one proved that GS+0x280/+0x288
//! is a 26-entry pool of 36-byte inline summary records, NOT the
//! owned-horse list. Decomp `FUN_140107510:156866-156869` and
//! `FUN_14010de40:160376` traverse the +0x438/+0x90 chain to get the
//! real owned vector<Horse*>.
//!
//! This test calls `gamestate.owned_horses` (mod-side traversal with
//! is_addr_readable guards at every hop) and reports count + per-horse
//! fields.
//!
//! Modes (env-driven):
//! - default: assert count > 0 and per-horse fields plausible.
//! - `HORSEY_EXPECT_OWNED=<N>`: strict; assert count == N. Use this
//!   when the save has a known horse count and we want regression
//!   coverage that the GS+0x438 chain still resolves correctly.

mod common;

use serde_json::{Value, json};

#[test]
fn owned_horses_are_findable() {
    let Some(game) = common::launch("find_owned_horses") else {
        return;
    };

    let resp = game
        .op_json("gamestate.owned_horses", &json!({}))
        .expect("gamestate.owned_horses should succeed");
    let result = resp.get("result").unwrap_or(&resp);

    let count = result
        .get("count")
        .and_then(Value::as_u64)
        .expect("response missing count") as usize;
    let horses = result
        .get("horses")
        .and_then(Value::as_array)
        .expect("response missing horses array");

    eprintln!("\n=== Owned-horse chain GS+0x438 -> +0x90 -> +0x130/+0x138 ===");
    eprintln!("count: {count}");
    for h in horses {
        eprintln!("  {h}");
    }

    game.log().event(
        "OWNED",
        &format!("owned_horse_count={count} via GS+0x438 chain"),
    );

    // We expect the chain to find a non-zero, plausible owned-horse
    // list. If count is 0 the chain is dead (likely no save loaded or
    // wrong sub-struct offset).
    assert!(
        count > 0,
        "owned-horse chain returned 0; either no save loaded, or the \
         GS+0x438 -> +0x90 -> +0x130/+0x138 chain needs different offsets"
    );
    assert!(
        count < 100,
        "owned-horse count {count} implausibly large; chain probably reading wrong vector"
    );

    // For each reported horse, age should be in [0, 200] and tired flags 0 or 1.
    for h in horses {
        let age = h.get("age").and_then(Value::as_i64).unwrap_or(-1);
        let tired_a = h.get("tired_a").and_then(Value::as_u64).unwrap_or(99);
        let tired_b = h.get("tired_b").and_then(Value::as_u64).unwrap_or(99);
        assert!(
            (0..=200).contains(&age),
            "horse {h} has implausible age {age}"
        );
        assert!(tired_a <= 1, "horse {h} has implausible tired_a {tired_a}");
        assert!(tired_b <= 1, "horse {h} has implausible tired_b {tired_b}");
    }

    if let Ok(expect_s) = std::env::var("HORSEY_EXPECT_OWNED") {
        let expect: usize = expect_s.parse().expect("HORSEY_EXPECT_OWNED not a number");
        assert_eq!(
            count, expect,
            "HORSEY_EXPECT_OWNED={expect}, but GS+0x438 chain reports {count}"
        );
    }

    game.pass(&format!("owned_horse_count={count}; chain validated"));
}
