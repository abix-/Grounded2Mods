//! Arm ALL green detour subsystems simultaneously and assert the
//! game still works. Catches inter-detour interference bugs that
//! single-subsystem tests would miss.
//!
//! Stack under arm:
//!   - D1 ext_genes (EVAL_DIPLOID_BLEND_A/B, GENE_ALLELE_SWAP)
//!   - D3.1/D3.2 lifecycle (HORSE_CONSTRUCTOR/DESTRUCTOR)
//!   - D3.4 combinator (GENE_COMBINATOR)
//!   - D5 render trampoline (APPLY_GENE_TO_HORSE)
//!
//! NOT armed (still red): D4 save sidecar. Addresses stale, would crash.
//!
//! Contract:
//!   1. Each subsystem arms in order, reporting armed: true.
//!   2. Idle 5s. Stats from each subsystem are sane (no runaway
//!      loops, no inter-subsystem counter mismatch).
//!   3. Disarm in reverse order. Each reports armed: false.
//!   4. Game still responds to ping after full disarm.
//!
//! This is the smoke test for "ship it" readiness. If this is
//! green, the green-subsystem stack is safe to arm in a real
//! gameplay session.

mod common;

use serde_json::{Value, json};
use std::thread;
use std::time::Duration;

fn as_u64(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(|x| x.as_u64())
}

fn armed_flag(resp: &Value, key: &str) -> bool {
    resp.get("result")
        .and_then(|r| r.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[test]
fn arm_all_green_subsystems_then_disarm_cleanly() {
    let Some(game) = common::launch("arm_full_safe_stack") else {
        return;
    };

    // ---------- arm in order ----------

    // D1 (ext_genes)
    let r = game
        .op_json("genes.ext.arm", &json!({}))
        .expect("genes.ext.arm");
    assert!(armed_flag(&r, "armed"), "D1 ext_genes arm failed: {r}");
    game.log().event("ARM", "D1 ext_genes armed");

    // D5 (render trampoline)
    let r = game
        .op_json("genes.ext.render.arm", &json!({}))
        .expect("render arm");
    assert!(armed_flag(&r, "armed"), "D5 render arm failed: {r}");
    game.log().event("ARM", "D5 render armed");

    // D3.1/D3.2 (lifecycle)
    let r = game
        .op_json("genes.ext.lifecycle.arm", &json!({}))
        .expect("lifecycle arm");
    assert!(
        armed_flag(&r, "armed"),
        "D3.1/D3.2 lifecycle arm failed: {r}"
    );
    game.log().event("ARM", "lifecycle armed");

    // D3.4 (combinator)
    let r = game
        .op_json("genes.ext.combinator.arm", &json!({}))
        .expect("combinator arm");
    assert!(armed_flag(&r, "armed"), "D3.4 combinator arm failed: {r}");
    game.log().event("ARM", "combinator armed");

    // ---------- idle ----------

    let idle = Duration::from_secs(5);
    game.log().event(
        "WAIT",
        &format!("idling {}s with full stack armed", idle.as_secs()),
    );
    thread::sleep(idle);

    // ---------- stats ----------

    let s_render = game
        .op_json("genes.ext.render.stats", &json!({}))
        .expect("render stats")
        .get("result")
        .cloned()
        .unwrap_or_default();
    let s_lifecycle = game
        .op_json("genes.ext.lifecycle.stats", &json!({}))
        .expect("lifecycle stats")
        .get("result")
        .cloned()
        .unwrap_or_default();
    let s_combinator = game
        .op_json("genes.ext.combinator.stats", &json!({}))
        .expect("combinator stats")
        .get("result")
        .cloned()
        .unwrap_or_default();

    let render_calls = as_u64(&s_render, "call_count").unwrap_or(u64::MAX);
    let ctor_calls = as_u64(&s_lifecycle, "ctor_calls").unwrap_or(u64::MAX);
    let dtor_calls = as_u64(&s_lifecycle, "dtor_calls").unwrap_or(u64::MAX);
    let entries_created = as_u64(&s_lifecycle, "entries_created").unwrap_or(u64::MAX);
    let entries_dropped = as_u64(&s_lifecycle, "entries_dropped").unwrap_or(u64::MAX);
    let combinator_calls = as_u64(&s_combinator, "call_count").unwrap_or(u64::MAX);

    game.log().event(
        "STATS",
        &format!(
            "render={render_calls} ctor={ctor_calls} dtor={dtor_calls} \
             entries+/- {entries_created}/{entries_dropped} \
             combinator={combinator_calls}"
        ),
    );

    // Sanity bounds.
    assert!(render_calls < 100_000_000, "render runaway: {render_calls}");
    assert!(ctor_calls < 1_000_000, "ctor runaway: {ctor_calls}");
    assert!(
        combinator_calls < 1_000_000,
        "combinator runaway: {combinator_calls}"
    );

    // Cross-subsystem invariants.
    //
    // RELAXED bounds: an end-to-end test arms AFTER game startup, so
    // some horses may already exist when lifecycle arms. Their
    // future dtor fires our handler but EXT_HORSE_GENOMES has no
    // entry to drop, so entries_dropped < dtor_calls. Similarly,
    // entries_created <= ctor_calls (a duplicate insert is a no-op).
    //
    // The strict invariant (`entries_created == ctor_calls`) only
    // holds when we arm BEFORE any horse is constructed; that case
    // is covered by `arm_lifecycle.rs` in isolation.
    assert!(
        entries_created <= ctor_calls,
        "entries_created ({entries_created}) > ctor_calls ({ctor_calls}); slow-path created spurious entry"
    );
    assert!(
        entries_dropped <= dtor_calls,
        "entries_dropped ({entries_dropped}) > dtor_calls ({dtor_calls}); slow-path double-dropped"
    );

    // ---------- disarm in reverse order ----------

    let r = game
        .op_json("genes.ext.combinator.disarm", &json!({}))
        .unwrap();
    assert!(!armed_flag(&r, "armed"), "combinator disarm failed: {r}");
    game.log().event("DISARM", "combinator disarmed");

    let r = game
        .op_json("genes.ext.lifecycle.disarm", &json!({}))
        .unwrap();
    assert!(
        !r.get("result")
            .and_then(|x| x.get("armed_ctor"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        "lifecycle ctor still armed: {r}"
    );
    assert!(
        !r.get("result")
            .and_then(|x| x.get("armed_dtor"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        "lifecycle dtor still armed: {r}"
    );
    game.log().event("DISARM", "lifecycle disarmed");

    let r = game.op_json("genes.ext.render.disarm", &json!({})).unwrap();
    assert!(!armed_flag(&r, "armed"), "render disarm failed: {r}");
    game.log().event("DISARM", "render disarmed");

    let r = game.op_json("genes.ext.disarm", &json!({})).unwrap();
    assert!(!armed_flag(&r, "armed"), "ext_genes disarm failed: {r}");
    game.log().event("DISARM", "ext_genes disarmed");

    // ---------- post-disarm ping ----------

    game.op_json("ping", &json!({}))
        .expect("game must still be alive after full disarm");
    game.pass("full safe stack arm + idle + disarm survived");
}
