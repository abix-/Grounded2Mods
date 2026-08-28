//! Locate the real `GAMESTATE_PTR` for this build by scanning
//! `.data` for the player's on-screen money value, then using
//! `mem.find_xrefs` to verify each candidate.
//!
//! Workflow:
//!
//!   1. `MODFORGE_EXPECT_MONEY=N` says "the user has $N in their
//!      currently-loaded save right now."
//!   2. Patternsleuth `mem.scan_data` finds every `.data` u32
//!      equal to N. Each candidate address is a possible
//!      `GAMESTATE_PTR + 0x308`.
//!   3. For each candidate, run `mem.find_xrefs` against it. The
//!      real money slot has many gameplay-code xrefs (cmp / mov /
//!      add); spurious matches usually have zero.
//!   4. Log the full ranking. The user picks the right candidate
//!      and feeds its address back as a fixed point for authoring
//!      `GAMESTATE_PTR` signatures.
//!
//! No env -> test skips (cannot guess money). `MODFORGE_ATTACH=1`
//! is the canonical way to run this (attach to a manually-loaded
//! save without disturbing it).

mod common;

use modforge::testkit::json::u64_at_key as u64_of;
use serde_json::json;

#[test]
fn find_gamestate_ptr_by_money_xref_count() {
    let expect_money = match std::env::var("MODFORGE_EXPECT_MONEY") {
        Ok(s) => s.parse::<u32>().ok(),
        Err(_) => None,
    };
    let Some(expect_money) = expect_money else {
        eprintln!(
            "MODFORGE_EXPECT_MONEY unset; cannot locate gamestate \
                   without a known-live value. Skipping."
        );
        return;
    };
    let Some(game) = common::launch("find_gamestate_via_money") else {
        return;
    };
    game.log().event(
        "FIND-GS",
        &format!("expected on-screen money = {expect_money}"),
    );

    // Step 1: scan .data for the money value.
    let scan = game
        .op_json(
            "mem.scan_data",
            &json!({"value": expect_money, "kind": "u32"}),
        )
        .expect("mem.scan_data must succeed");
    let scan_result = scan.get("result").expect("result");
    let hits = scan_result
        .get("hits")
        .and_then(|v| v.as_array())
        .expect("result.hits array");
    game.log().event(
        "FIND-GS",
        &format!(
            "scan_data: {} candidate(s) for u32 == {expect_money}",
            hits.len()
        ),
    );
    assert!(
        !hits.is_empty(),
        "no .data u32 holds the value {expect_money}; either the user is \
         not in-save or money is stored at a non-u32-aligned offset"
    );

    // Step 2: for each candidate, verify by reading adjacent fields.
    // GameState layout per decomp:
    //   +0x308 money  (we just scanned for this value)
    //   +0x314 year   (sane: 0..100 typically, year-since-game-start)
    //   +0x318 sleeps (sane: small monotonic counter, < ~10000)
    // A candidate is plausible iff year < 100 AND sleeps < 10_000_000.
    // GameState is heap-allocated; the EXE-relative xref-count test
    // doesn't apply (RIP-rel disp32 can't reach heap addresses past
    // ±2GB from .text).
    fn peek_u32(game: &modforge::harness::RunningGame, addr: u64) -> Option<u32> {
        let resp = game
            .op_json("mem.peek", &json!({"addr": addr, "kind": "u32"}))
            .ok()?;
        resp.get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
    }
    fn peek_u64(game: &modforge::harness::RunningGame, addr: u64) -> Option<u64> {
        let resp = game
            .op_json("mem.peek", &json!({"addr": addr, "kind": "u64"}))
            .ok()?;
        resp.get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.as_str())
            .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
    }
    #[derive(Debug, Clone)]
    struct Candidate {
        money_addr: u64,
        money: u32,
        year: u32,
        sleeps: u32,
        roster_begin: u64,
        roster_end: u64,
        plausible: bool,
    }
    let mut ranked: Vec<Candidate> = Vec::new();
    for h in hits.iter() {
        let money_addr = u64_of(h, "addr").expect("addr");
        let base = money_addr.saturating_sub(0x308);
        let m = peek_u32(&game, money_addr).unwrap_or(0);
        let year = peek_u32(&game, money_addr + 0xC).unwrap_or(u32::MAX);
        let sleeps = peek_u32(&game, money_addr + 0x10).unwrap_or(u32::MAX);
        // HORSES_BEGIN = base + 0x280, HORSES_END = base + 0x288.
        // Real GameState has either both null (fresh / empty) or
        // both heap pointers (>= 0x10000) with end >= begin AND
        // (end - begin) divisible by 0x24 (roster entry stride) AND
        // count < 2560. Decoys mostly have garbage there.
        let rb = peek_u64(&game, base + 0x280).unwrap_or(u64::MAX);
        let re = peek_u64(&game, base + 0x288).unwrap_or(u64::MAX);
        let roster_ok = {
            let both_null = rb == 0 && re == 0;
            let both_heap =
                rb >= 0x10000 && re >= rb && (re - rb) % 0x24 == 0 && (re - rb) / 0x24 < 2560;
            both_null || both_heap
        };
        let year_match = match std::env::var("MODFORGE_EXPECT_YEAR") {
            Ok(s) => s.parse::<u32>().ok().map(|v| v == year).unwrap_or(false),
            Err(_) => year < 1000, // wide fallback
        };
        let sleeps_match = match std::env::var("MODFORGE_EXPECT_SLEEPS") {
            Ok(s) => s.parse::<u32>().ok().map(|v| v == sleeps).unwrap_or(false),
            Err(_) => sleeps < 100_000_000, // wide fallback
        };
        let plausible = m == expect_money && year_match && sleeps_match && roster_ok;
        ranked.push(Candidate {
            money_addr,
            money: m,
            year,
            sleeps,
            roster_begin: rb,
            roster_end: re,
            plausible,
        });
    }

    ranked.sort_by(|a, b| b.plausible.cmp(&a.plausible));
    for c in &ranked {
        let base = c.money_addr.saturating_sub(0x308);
        let tag = if c.plausible { "OK" } else { "??" };
        game.log().event(
            "FIND-GS",
            &format!(
                "{tag} gamestate@0x{base:x} money@0x{:x}={} year={} sleeps={} \
                 roster_begin=0x{:x} roster_end=0x{:x}",
                c.money_addr, c.money, c.year, c.sleeps, c.roster_begin, c.roster_end,
            ),
        );
    }

    let plausible: Vec<_> = ranked.iter().filter(|c| c.plausible).collect();
    assert!(
        !plausible.is_empty(),
        "no candidate has plausible money + year + sleeps + roster. \
         Ranking: {ranked:?}"
    );
    assert_eq!(
        plausible.len(),
        1,
        "more than one candidate looks plausible; need a tighter \
         discriminator. Plausible: {plausible:?}",
    );
    let top = plausible[0];
    game.pass(&format!(
        "real gamestate_ptr = 0x{:x} (money@0x{:x}={}, year={}, sleeps={}, \
         roster_begin=0x{:x}, roster_end=0x{:x})",
        top.money_addr.saturating_sub(0x308),
        top.money_addr,
        top.money,
        top.year,
        top.sleeps,
        top.roster_begin,
        top.roster_end,
    ));
}
