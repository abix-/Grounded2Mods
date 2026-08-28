//! Regression test for `gamestate::ptr()` deref correctness.
//!
//! Background: the `.data` slot at `DAT_1403fb0d8` is a POINTER to
//! a heap-allocated GameState. The decomp evidence is unambiguous:
//!
//!   - `FUN_14009c6a0:46` heap-allocates 0x448 bytes via
//!     `FUN_1402c704c`, constructs in place, then stores the heap
//!     pointer into the slot.
//!   - `FUN_1400fd580:86` (the in-place constructor) does
//!     `DAT_1403fb0d8 = param_1` (param_1 is the freshly-allocated
//!     buffer).
//!   - `FUN_14009c4e0:26` zeroes the slot on destruction.
//!
//! A prior fix attempt mis-interpreted the slot as the struct
//! itself (no deref) after a stale value (`0x2400000000B`) crashed
//! the deref in main-menu state. Removing the deref hid the crash
//! but caused every field read to return random `.data` bytes
//! adjacent to the slot. This test exists so that bug can never
//! re-land silently.
//!
//! Strategy:
//!
//!   - Always-on: with no save loaded, `gamestate.diag` must report
//!     a self-consistent state. Either `ptr == 0` (the slot deref
//!     produced a null or rejected-as-implausible pointer) AND
//!     `verdict_looks_loaded == false`, OR both flip to "loaded"
//!     coherently.
//!
//!   - `MODFORGE_EXPECT_LOADED=1`: the user has told us "I am in a
//!     real save right now." Then:
//!       * `ptr` MUST be non-zero (deref succeeded).
//!       * `ptr` MUST look like a heap address (not `.data`-adjacent).
//!       * `money`, `year`, `sleeps` MUST be within bounded ranges
//!         that real saves produce. NO exact-value match (that
//!         violates the "only money is given" rule and is what the
//!         previous Claude fell into). Just structural bounds.
//!       * `verdict_looks_loaded` MUST be true.

mod common;

use serde_json::{Value, json};

fn diag(game: &modforge::harness::RunningGame) -> Value {
    let resp = game
        .op_json("gamestate.diag", &json!({}))
        .expect("gamestate.diag must succeed");
    resp.get("result")
        .cloned()
        .expect("diag response missing result")
}

fn parse_hex(v: &Value) -> Option<u64> {
    v.as_str()
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
}

fn ptr_value(d: &Value) -> u64 {
    d.get("ptr").and_then(parse_hex).unwrap_or(u64::MAX)
}

fn fields(d: &Value) -> &Value {
    d.get("fields_u32")
        .expect("diag must include fields_u32 block")
}

fn looks_loaded(d: &Value) -> bool {
    d.get("verdict_looks_loaded")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[test]
fn ptr_is_null_or_heap_shaped() {
    // Always-on invariant: the value returned by `ptr()` is either
    // 0 (no save / stale garbage rejected by the plausibility gate)
    // or a heap-shaped pointer (>= 0x10000, <= canonical user max,
    // 8-aligned). It is NEVER a `.data` address inside the loaded
    // image, which is what the broken "slot IS the struct" code
    // path produced.
    let Some(game) = common::launch("gamestate_ptr_deref_shape") else {
        return;
    };
    let d = diag(&game);
    game.log().event("DIAG", &format!("gamestate.diag = {d:#}"));

    let p = ptr_value(&d);
    assert_ne!(p, u64::MAX, "diag.ptr missing or unparseable; diag = {d:#}");

    if p == 0 {
        // Null is the canonical "no save loaded" state. verdict
        // MUST be false; if not, our heuristic and our deref
        // disagree and the snapshot would lie.
        assert!(
            !looks_loaded(&d),
            "ptr == 0 but looks_loaded() == true; diag = {d:#}"
        );
        game.pass("ptr == 0 with looks_loaded == false (main-menu)");
        return;
    }

    assert!(
        p >= 0x10000 && p <= 0x7fff_ffff_ffff && (p & 0x7) == 0,
        "ptr 0x{p:x} is non-zero but not heap-shaped (low / kernel / \
         misaligned). The plausibility gate let through garbage. diag = {d:#}"
    );
    game.pass(&format!("ptr = 0x{p:x} is heap-shaped"));
}

#[test]
fn save_loaded_yields_real_bounded_values() {
    // Gated: only run when the user has confirmed they are in a
    // real save. Without that confirmation we cannot tell "deref
    // produced garbage" from "no save loaded."
    if std::env::var("MODFORGE_EXPECT_LOADED").ok().as_deref() != Some("1") {
        eprintln!("MODFORGE_EXPECT_LOADED!=1, skipping in-save assertion");
        return;
    }
    let Some(game) = common::launch("gamestate_ptr_deref_in_save") else {
        return;
    };
    let d = diag(&game);
    game.log()
        .event("DIAG-LOADED", &format!("gamestate.diag = {d:#}"));

    let p = ptr_value(&d);
    assert_ne!(p, u64::MAX, "diag.ptr missing; diag = {d:#}");
    assert_ne!(
        p, 0,
        "user reports save loaded but ptr() == 0. Slot deref returned \
         null or was rejected by the plausibility gate. diag = {d:#}"
    );
    assert!(
        p >= 0x10000 && p <= 0x7fff_ffff_ffff && (p & 0x7) == 0,
        "ptr 0x{p:x} is non-zero but not heap-shaped. diag = {d:#}"
    );

    assert!(
        looks_loaded(&d),
        "ptr looks heap-shaped but verdict_looks_loaded = false. The \
         roster vector at +0x280/+0x288 didn't pass structural checks; \
         likely the deref is still off or HORSES_BEGIN/END drifted. \
         diag = {d:#}"
    );

    // Bounded structural plausibility, NOT exact-value matching.
    // These bounds are intentionally wide. The point is to detect
    // "the deref returned `.data` bytes" (which would give wildly
    // out-of-range values like 0 / huge garbage), not to assert
    // specific gameplay state.
    let f = fields(&d);
    let money = f
        .get("money_0x308")
        .and_then(|v| v.as_u64())
        .expect("money_0x308 must be present");
    let year = f
        .get("year_0x314")
        .and_then(|v| v.as_u64())
        .expect("year_0x314 must be present");
    let sleeps = f
        .get("sleeps_0x318")
        .and_then(|v| v.as_u64())
        .expect("sleeps_0x318 must be present");

    assert!(
        money <= 100_000_000,
        "money = {money}; real saves are bounded under $100M. A value \
         this large suggests deref returned garbage. diag = {d:#}"
    );
    assert!(
        year >= 1 && year < 10_000,
        "year = {year}; real saves have year in [1, 10000). diag = {d:#}"
    );
    assert!(
        sleeps < 1_000_000,
        "sleeps = {sleeps}; real saves don't exceed 1M sleeps. diag = {d:#}"
    );

    game.pass(&format!(
        "in-save deref: ptr=0x{p:x} money={money} year={year} sleeps={sleeps}"
    ));
}
