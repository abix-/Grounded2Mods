//! Thin wrapper over [`modforge::testkit::assets::run`].

mod common;

use modforge::testkit::assets;

#[test]
fn find_asset_next_to_exe() {
    let Some(game) = common::launch("find_asset") else {
        return;
    };
    let cfg = assets::Config::from_env("HORSEY_ASSET", "genes.xml");
    let found = assets::run(&game, &cfg).unwrap_or_else(|e| panic!("{e}"));
    game.pass(&format!(
        "{} at {} ({} bytes)",
        cfg.asset_name,
        found.path.display(),
        found.size
    ));
}
