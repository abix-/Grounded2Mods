//! Thin wrapper over [`modforge::testkit::xrefs::run`]. Default target
//! is the resolved GAMESTATE_PTR slot for horsey-mod regression cover.

mod common;

use modforge::testkit::xrefs;
use serde_json::{Value, json};

#[test]
fn find_xrefs_locates_known_gamestate_references() {
    let Some(game) = common::launch("find_xrefs") else {
        return;
    };

    let default_target = || -> u64 {
        let resp = game
            .op_json("targets.resolve.gamestate_ptr", &json!({}))
            .expect("targets.resolve.gamestate_ptr");
        resp.get("result")
            .and_then(|r| r.get("slot"))
            .and_then(Value::as_str)
            .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .expect("GAMESTATE_PTR slot")
    };
    let cfg = match std::env::var("HORSEY_XREF_TARGET") {
        Ok(_) => xrefs::Config::from_env("HORSEY_XREF", None).expect("env"),
        Err(_) => xrefs::Config::from_env("HORSEY_XREF", Some(default_target())).expect("env"),
    };

    let res = xrefs::run(&game, &cfg).unwrap_or_else(|e| panic!("{e}"));
    game.pass(&format!(
        "find_xrefs: {} hit(s) to 0x{:x}",
        res.hits.len(),
        res.target_addr
    ));
}
