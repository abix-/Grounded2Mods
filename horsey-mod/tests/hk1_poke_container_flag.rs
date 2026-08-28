//! HK1 Stage S0.5: hypothesis test for the container flag.
//!
//! Diff of tomtato bytes in truck vs pasture showed only 3 changed
//! u32 fields:
//!   +0x008 (ticker, ignore)
//!   +0x1d0  truck=7  pasture=9  <-- CANDIDATE container kind
//!   +0x1dc  truck=36 pasture=20  (sub-state, maybe slot index)
//!
//! This test reads +0x1d0 + +0x1dc on the target horse, then writes
//! TRUCK_GUESS=7 to +0x1d0 (overwriting whatever's there). User should
//! observe tomtato visibly moving (if the hypothesis is right).
//!
//! Env:
//!   HORSEY_HORSE=<name>  default "tomtato"
//!   HK1_TARGET=truck|pasture|none  default "truck" (writes 7);
//!                                  "pasture" writes 9; "none" no-op
//!                                  (read-only).

mod common;

use serde_json::{Value, json};

#[test]
fn poke_container_flag() {
    let Some(game) = common::launch("hk1_poke_container_flag") else {
        return;
    };
    let want = std::env::var("HORSEY_HORSE").unwrap_or_else(|_| "tomtato".to_string());
    eprintln!("[GATE] waiting for '{want}' to appear in owned list (up to 180s) - load your save");
    // Set HORSEY_HORSE so the helper waits for the exact horse we need.
    if std::env::var("HORSEY_HORSE").is_err() {
        unsafe {
            std::env::set_var("HORSEY_HORSE", &want);
        }
    }
    let h = common::wait_for_target_horse(&game, std::time::Duration::from_secs(180));
    let horse_ptr = h.id;
    eprintln!("\nhorse '{}' at {}", h.name, h.ptr_s);

    let off_kind = 0x1d0u64;
    let off_sub = 0x1dcu64;
    let addr_kind = horse_ptr + off_kind;
    let addr_sub = horse_ptr + off_sub;

    let r_kind = game
        .op_json("mem.peek", &json!({"addr": addr_kind, "kind": "u32"}))
        .unwrap();
    let r_sub = game
        .op_json("mem.peek", &json!({"addr": addr_sub,  "kind": "u32"}))
        .unwrap();
    let before_kind = r_kind
        .get("result")
        .and_then(|r| r.get("value"))
        .and_then(Value::as_u64);
    let before_sub = r_sub
        .get("result")
        .and_then(|r| r.get("value"))
        .and_then(Value::as_u64);
    eprintln!("BEFORE  +0x1d0={before_kind:?}  +0x1dc={before_sub:?}");

    let target = std::env::var("HK1_TARGET").unwrap_or_else(|_| "truck".to_string());
    let write_val: Option<u64> = match target.as_str() {
        "truck" => Some(7),
        "pasture" => Some(9),
        "none" => None,
        other => panic!("HK1_TARGET must be truck|pasture|none, got '{other}'"),
    };

    if let Some(v) = write_val {
        eprintln!("WRITING +0x1d0 = {v} (target={target})");
        let resp = game
            .op_json(
                "mem.poke",
                &json!({
                    "addr": addr_kind, "kind": "u32", "value": v,
                }),
            )
            .expect("mem.poke");
        eprintln!("poke response: {resp}");
    } else {
        eprintln!("HK1_TARGET=none, no write performed");
    }

    let r_kind2 = game
        .op_json("mem.peek", &json!({"addr": addr_kind, "kind": "u32"}))
        .unwrap();
    let r_sub2 = game
        .op_json("mem.peek", &json!({"addr": addr_sub,  "kind": "u32"}))
        .unwrap();
    let after_kind = r_kind2
        .get("result")
        .and_then(|r| r.get("value"))
        .and_then(Value::as_u64);
    let after_sub = r_sub2
        .get("result")
        .and_then(|r| r.get("value"))
        .and_then(Value::as_u64);
    eprintln!("AFTER   +0x1d0={after_kind:?}  +0x1dc={after_sub:?}");

    // Pause so the user can visually verify the in-game effect of the
    // poke before the harness tears the game down. HK1_OBSERVE_SECS=0
    // skips the pause.
    let obs: u64 = std::env::var("HK1_OBSERVE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(90);
    if obs > 0 {
        eprintln!(
            ">>> OBSERVE NOW: did '{}' visibly move? (sleeping {obs}s)",
            h.name
        );
        std::thread::sleep(std::time::Duration::from_secs(obs));
    }

    game.pass(&format!(
        "container probe: before(kind={before_kind:?}, sub={before_sub:?}) -> after(kind={after_kind:?}, sub={after_sub:?})"
    ));
}
