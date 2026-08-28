//! `horsey-play`. Easy-button: build horsey-mod, launch Horsey via
//! Steam, inject horsey.dll, wait for the HTTP plane, then EXIT
//! leaving the game running for normal play.
//!
//! Unlike the test harness, this binary calls `mem::forget` on the
//! `RunningGame` handle so the Drop impl (which taskkill's Horsey.exe)
//! never runs. The game stays alive after this binary returns.
//!
//! Usage:
//!   cargo run -p horsey-mod --bin horsey-play --release
//!
//! Env vars (inherited from modforge::harness):
//!   MODFORGE_SKIP_BUILD=1   skip cargo build
//!   MODFORGE_NO_GAME=1      skip launch entirely (no-op exit)
//!   MODFORGE_STEAM_EXE=...  override steam.exe path

use modforge::harness::{BuildDef, GameDef, GameHarness, HttpProbe, InjectorDef};
use std::path::{Path, PathBuf};
use std::time::Duration;

const STEAM_APP_ID: u32 = 3602570;
const PROCESS_NAME: &str = "Horsey.exe";
const HTTP_PORT: u16 = 33077;
const HTTP_ENDPOINT: &str = "/op";

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .expect("horsey-mod has no parent dir")
        .to_path_buf()
}

fn injector_exe(release: bool) -> PathBuf {
    let profile = if release { "release" } else { "debug" };
    workspace_root()
        .join("target")
        .join("x86_64-pc-windows-msvc")
        .join(profile)
        .join("horsey-inject.exe")
}

fn main() -> anyhow::Result<()> {
    let release = cfg!(not(debug_assertions));

    let mut spec = GameDef::defaults(
        STEAM_APP_ID,
        PROCESS_NAME,
        HttpProbe::new(HTTP_PORT, HTTP_ENDPOINT),
    );
    let mut inj = InjectorDef::new(injector_exe(release));
    inj.args.push("--fresh".into());
    spec.injector = Some(inj);
    spec.build = Some(BuildDef {
        package: "horsey-mod",
        release,
        workspace_dir: workspace_root(),
    });
    spec.launch_timeout = Duration::from_secs(180);

    println!("horsey-play: building + launching + injecting (release={release})");
    let game = GameHarness::launch(&spec, "play")?;
    println!("horsey-play: game running, mod injected, HTTP ready on 127.0.0.1:{HTTP_PORT}");
    println!("horsey-play: exiting; game stays up. Have fun.");

    // Leak the handle so Drop never runs and the game keeps living.
    std::mem::forget(game);
    Ok(())
}
