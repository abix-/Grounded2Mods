//! Thin wrapper over [`modforge::testkit::build_info::run`].

mod common;

use modforge::testkit::build_info;

#[test]
fn build_info_is_well_formed() {
    let Some(game) = common::launch("build_info") else {
        return;
    };
    let mut cfg = build_info::Config::from_env("HORSEY_BUILD");
    // Horsey-specific default expectation: exe_path contains "horsey".
    if cfg.expect_exe_path_contains.is_none() {
        cfg.expect_exe_path_contains = Some("horsey".into());
    }
    let bi = build_info::run(&game, &cfg).unwrap_or_else(|e| panic!("{e}"));
    game.pass(&format!(
        "build_info OK: image_base=0x{:x} size={}",
        bi.image_base, bi.exe_size
    ));
}
