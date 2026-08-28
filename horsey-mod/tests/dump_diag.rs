//! Thin wrapper over [`modforge::testkit::op::run`]. See that module
//! for the env-var contract.

mod common;

use modforge::testkit::op;

#[test]
fn dump_op() {
    let Some(game) = common::launch("dump_op") else {
        return;
    };
    let cfg = op::Config::from_env("HORSEY_DUMP", "gamestate.diag").expect("HORSEY_DUMP_* env");
    op::run(&game, &cfg).unwrap_or_else(|e| panic!("{e}"));
    game.pass(&format!("op {} returned successfully", cfg.op));
}
