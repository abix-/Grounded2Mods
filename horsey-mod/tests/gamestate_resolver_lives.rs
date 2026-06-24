//! Regression: the GAMESTATE_PTR resolver finds the live slot, its
//! deref looks like a real save, and owned_horses populates. Trips
//! fast on the next slot drift across Horsey builds; the 2026-05-17
//! +0x1110 drift silently broke every gamestate-dependent op for a
//! whole session before this test existed.
//!
//! Default: fresh launch via the harness (canonical contract). Per
//! the skill, the game auto-loads the save on every launch, so
//! gamestate is live by the time HTTP comes up. To probe an
//! already-running session interactively, set `MODFORGE_ATTACH=1`.

mod common;

use serde_json::{json, Value};

#[test]
fn gamestate_resolver_is_alive() {
    let Some(game) = common::launch("gamestate_resolver_lives") else { return };

    // The save auto-loads on launch, but the HTTP plane comes up
    // DURING the load: there is a brief window where GAMESTATE_PTR's
    // slot is still null (deref == 0x0) because the world has not been
    // deserialized into it yet. Poll until the gamestate populates so
    // this gate exercises the RESOLVER, not the load race. Querying at
    // t=0 reads the empty slot and false-fails. That exact early
    // read got misdiagnosed as an address drift once.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut v = game.op_json("targets.resolve.gamestate_ptr", &json!({})).expect("resolve op");
    loop {
        let loaded = v
            .get("result")
            .unwrap_or(&v)
            .get("money_at_deref_plus_0x308")
            .map(|m| !m.is_null())
            .unwrap_or(false);
        if loaded || std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
        v = game.op_json("targets.resolve.gamestate_ptr", &json!({})).expect("resolve op");
    }
    let r = v.get("result").unwrap_or(&v);
    eprintln!("[RESOLVE] {}", serde_json::to_string_pretty(r).unwrap());

    let deref = r.get("deref").and_then(Value::as_str).unwrap_or("");
    let money = r.get("money_at_deref_plus_0x308");
    assert!(
        money.map(|m| !m.is_null()).unwrap_or(false),
        "money_at_deref_plus_0x308 still null after 30s -- resolver broken. deref={deref}"
    );

    let owned = common::list_owned(&game);
    eprintln!("[OWNED] {} horse(s)", owned.len());
    for h in &owned {
        let n = h.get("name").and_then(Value::as_str).unwrap_or("?");
        eprintln!("  - {n}");
    }
    assert!(!owned.is_empty(), "owned_horses empty -- GS deref worked but horse list reader broke");
}
