//! Thin wrapper over [`modforge::testkit::recipes::find_fn_by_rdata_string`].
//! Default string sig is "retired %s (useless)" for backward compat.

mod common;

use modforge::testkit::recipes;

const DEFAULT_STRING_HEX: &str = "72 65 74 69 72 65 64 20 25 73 20 28 75 73 65 6c 65 73 73 29"; // "retired %s (useless)"

#[test]
fn locate_fn_by_rdata_string() {
    let Some(game) = common::launch("locate_fn_by_rdata_string") else {
        return;
    };
    let mut cfg = recipes::FindFnByStringConfig::from_env("HORSEY_FN", DEFAULT_STRING_HEX)
        .expect("HORSEY_FN_* env");
    // The historical "retired %s (useless)" anchor wants lookback=3
    // (the format string actually starts with "%s " before "retired").
    if std::env::var("HORSEY_FN_STRING_LOOKBACK").is_err() && cfg.string_sig == DEFAULT_STRING_HEX {
        cfg.string_lookback = 3;
    }
    let entries = recipes::find_fn_by_rdata_string(&game, &cfg).unwrap_or_else(|e| panic!("{e}"));
    game.pass(&format!("{} candidate fn entry(ies) found", entries.len()));
}
