//! Shared test harness for horsey-mod integration tests.
//!
//! Each test launches a fresh Horsey instance via `modforge::harness`,
//! injects `horsey.dll`, and tears down on Drop. Run with
//! `--test-threads=1` so tests don't fight over the single game
//! process.
//!
//! Env vars:
//! - `MODFORGE_NO_GAME=1` skips the launch; tests return cleanly.
//! - `MODFORGE_SKIP_BUILD=1` skips `cargo build` for fast iteration.
//! - `MODFORGE_STEAM_EXE=<path>` overrides the Steam executable
//!   path (default `C:\Games\Steam\steam.exe`).

#![allow(dead_code)]

use modforge::harness::{
    BuildDef, GameHarness, GameDef, HttpProbe, InjectorDef, RunningGame,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Horsey on Steam.
const STEAM_APP_ID: u32 = 3602570;
const PROCESS_NAME: &str = "Horsey.exe";
const HTTP_PORT: u16 = 33077;
const HTTP_ENDPOINT: &str = "/op";

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at horsey-mod; parent is the
    // workspace root.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .expect("horsey-mod has no parent dir")
        .to_path_buf()
}

fn injector_exe() -> PathBuf {
    // cargo build puts the injector at
    // target/x86_64-pc-windows-msvc/debug/horsey-inject.exe.
    workspace_root()
        .join("target")
        .join("x86_64-pc-windows-msvc")
        .join("debug")
        .join("horsey-inject.exe")
}

pub fn spec() -> GameDef {
    let mut spec = GameDef::defaults(
        STEAM_APP_ID,
        PROCESS_NAME,
        HttpProbe::new(HTTP_PORT, HTTP_ENDPOINT),
    );
    let mut inj = InjectorDef::new(injector_exe());
    // Always start from a clean injector state for tests. Without
    // this, horsey-inject sees the previous run's `.injstate` and
    // refuses with "use --reload to swap, or --fresh to ignore".
    inj.args.push("--fresh".into());
    spec.injector = Some(inj);
    spec.build = Some(BuildDef {
        package: "horsey-mod",
        release: false,
        workspace_dir: workspace_root(),
    });
    spec.launch_timeout = Duration::from_secs(180);
    spec
}

/// Launch a fresh game + inject the mod + wait for HTTP ready.
/// `test_name` is used to name the per-run log file under
/// `target/test-runs/`.
///
/// Returns `None` if `MODFORGE_NO_GAME` is set so callers can
/// `let Some(game) = common::launch("smoke") else { return; };` cleanly.
pub fn launch(test_name: &str) -> Option<RunningGame> {
    if modforge::harness::should_skip() {
        eprintln!("MODFORGE_NO_GAME=1, skipping live-game test");
        return None;
    }
    let s = spec();
    if std::env::var("MODFORGE_ATTACH").ok().as_deref() == Some("1") {
        eprintln!("MODFORGE_ATTACH=1: attaching to already-running game");
        return match GameHarness::attach_existing(&s, test_name) {
            Ok(g) => Some(g),
            Err(e) => panic!("attach_existing failed: {e}"),
        };
    }
    match GameHarness::launch(&s, test_name) {
        Ok(g) => Some(g),
        Err(e) => panic!("harness launch failed: {e}"),
    }
}

// ===== Generic test helpers (think powershell cmdlets) =====
//
// Tests are parameterized via HORSEY_* env vars instead of hardcoded
// names. A test that needs a horse calls `target_horse(&game)`; that
// reads `HORSEY_HORSE` (name, case-insensitive) and returns the
// matching owned horse. With no env set, defaults to the first owned
// horse so a bare `cargo test` still runs.

use serde_json::{json, Value};

/// One owned horse as returned by `gamestate.owned_horses`.
#[derive(Debug, Clone)]
pub struct TargetHorse {
    pub name:  String,
    pub ptr_s: String,
    pub id:    u64,
}

/// Fetch the full owned-horse list from the running game.
pub fn list_owned(game: &RunningGame) -> Vec<Value> {
    let r = game.op_json("gamestate.owned_horses", &json!({})).unwrap_or(Value::Null);
    r.get("result").and_then(|x| x.get("horses"))
        .and_then(Value::as_array).cloned().unwrap_or_default()
}

/// Find an owned horse by case-insensitive name.
pub fn find_owned(game: &RunningGame, name: &str) -> Option<TargetHorse> {
    let want = name.to_ascii_lowercase();
    list_owned(game).into_iter().find_map(|h| {
        let n = h.get("name").and_then(Value::as_str)?.to_string();
        if n.to_ascii_lowercase() != want { return None; }
        let ptr_s = h.get("ptr").and_then(Value::as_str)?.to_string();
        let id = u64::from_str_radix(ptr_s.trim_start_matches("0x"), 16).ok()?;
        Some(TargetHorse { name: n, ptr_s, id })
    })
}

/// Pick the target horse for a test. Resolution order:
/// 1. `HORSEY_HORSE` env var -> exact name match.
/// 2. First owned horse in the roster.
///
/// Panics if neither yields a horse.
pub fn target_horse(game: &RunningGame) -> TargetHorse {
    if let Ok(name) = std::env::var("HORSEY_HORSE") {
        return find_owned(game, &name)
            .unwrap_or_else(|| panic!("HORSEY_HORSE='{name}' not in owned list"));
    }
    let h = list_owned(game).into_iter().next()
        .expect("no owned horses; load a save first");
    let name = h.get("name").and_then(Value::as_str).unwrap_or("?").to_string();
    let ptr_s = h.get("ptr").and_then(Value::as_str).unwrap_or("0x0").to_string();
    let id = u64::from_str_radix(ptr_s.trim_start_matches("0x"), 16).unwrap_or(0);
    TargetHorse { name, ptr_s, id }
}

/// Parse an env var as `T`, falling back to `default` if unset.
/// Panics if the var is set but unparseable.
pub fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T
where
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(v) => v.parse().unwrap_or_else(|e| panic!("{key} parse: {e}")),
        Err(_) => default,
    }
}

/// Horsey-specific resolver for `modforge::testkit::watch` base specs.
/// Accepts:
/// - `horse_ptr`  -> [`target_horse(game).id`]
/// - `gamestate`  -> live `gamestate.diag.ptr`
/// - `0x<hex>`    -> raw address
///
/// Use by passing this into `EnvRegionConfig::resolve` (or the single
/// variant) when building a watch test.
pub fn resolve_base(game: &RunningGame, spec: &str) -> u64 {
    if let Some(hex) = spec.strip_prefix("0x").or_else(|| spec.strip_prefix("0X")) {
        return u64::from_str_radix(hex, 16)
            .unwrap_or_else(|e| panic!("base raw hex parse: {e}"));
    }
    match spec {
        "horse_ptr" => target_horse(game).id,
        "gamestate" => {
            let r = game.op_json("gamestate.diag", &json!({}))
                .unwrap_or_else(|e| panic!("gamestate.diag failed: {e}"));
            let ptr_s = r.get("result").and_then(|x| x.get("ptr")).and_then(Value::as_str)
                .unwrap_or_else(|| panic!("gamestate.diag returned no ptr: {r}"));
            let h = ptr_s.trim_start_matches("0x").trim_start_matches("0X");
            let p = u64::from_str_radix(h, 16)
                .unwrap_or_else(|e| panic!("gamestate.diag ptr parse: {e}"));
            assert!(p != 0, "gamestate.diag returned ptr=0; load a save first");
            p
        }
        other => panic!("base must be horse_ptr | gamestate | 0x<hex>, got {other:?}"),
    }
}

/// Path to `menu_targets.json` next to the staged horsey.dll. Keyed
/// captures of screen coordinates that automated tests need at a
/// fresh save-load (e.g. the house door on the world map).
pub fn menu_targets_path() -> PathBuf {
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    workspace_root()
        .join("target")
        .join("x86_64-pc-windows-msvc")
        .join(profile)
        .join("menu_targets.json")
}

fn read_menu_target(key: &str) -> Option<(i32, i32)> {
    let s = std::fs::read_to_string(menu_targets_path()).ok()?;
    let v: Value = serde_json::from_str(&s).ok()?;
    let o = v.get(key)?;
    let x = o.get("x")?.as_i64()? as i32;
    let y = o.get("y")?.as_i64()? as i32;
    Some((x, y))
}

/// Read `active_scene_id` (GS+0x25C) via the existing
/// `hk1.probe.active_location` op. Returns the i32 the game stores,
/// or `None` if the op failed or omitted the field.
pub fn active_scene_id(game: &RunningGame) -> Option<i32> {
    let v = game.op_json("hk1.probe.active_location", &json!({})).ok()?;
    let r = v.get("result").unwrap_or(&v);
    r.get("active_scene_id").and_then(Value::as_i64).map(|n| n as i32)
}

/// HK1 prereq: block until the Home Location scene is active. On
/// fresh save-load the game spawns the player in the truck on the
/// world map (active_scene_id != Home). The Home LOC at slot 0x00 of
/// the scene table EXISTS in memory regardless (it holds the owned
/// horse list), so `hk1.read_cursor` is NOT a valid gate. Instead we
/// snapshot `active_scene_id` before clicking the house door, then
/// poll until it changes.
///
/// Click target: `menu_targets.json` key `home_door_from_truck_spawn`,
/// captured via `tests/hk1_capture_house_coords.rs` while the cursor
/// hovers the door on the world map.
///
/// Panics if the target entry is missing or the scene never
/// transitions within `timeout`.
pub fn ensure_home_scene_loaded(game: &RunningGame, timeout: std::time::Duration) {
    // 1. Wait for the save to finish loading via the proven horse-roster
    //    poll. If owned_horses populates, GS is alive.
    let h = wait_for_target_horse(game, std::time::Duration::from_secs(120));
    eprintln!("[home-scene] save loaded; first owned horse = {} @ {}", h.name, h.ptr_s);

    // 2. Read active_scene_id baseline. If it's None even after save
    //    load, dump diagnostic state and bail with actionable info.
    let baseline = match active_scene_id(game) {
        Some(id) => id,
        None => {
            let diag = game.op_json("hk1.probe.active_location", &json!({}))
                .map(|v| v.to_string()).unwrap_or_else(|e| format!("(probe err: {e})"));
            panic!(
                "ensure_home_scene_loaded: save loaded but active_scene_id is None. \
                 Either the value at GS+0x25C is outside [-1, 256) (need to widen the \
                 filter in gamestate::active_scene_id) or the gate signal is wrong. \
                 probe.active_location = {diag}"
            );
        }
    };
    eprintln!("[home-scene] baseline active_scene_id = {baseline}");

    let (hx, hy) = read_menu_target("home_door_from_truck_spawn").unwrap_or_else(|| {
        panic!(
            "menu_targets.json missing 'home_door_from_truck_spawn'; run \
             `MODFORGE_ATTACH=1 cargo test -p horsey-mod --test \
             hk1_capture_house_coords -- --nocapture` with the cursor over the house door first"
        )
    });
    eprintln!("[home-scene] clicking house door @ ({hx}, {hy}) backend=l1");
    if let Err(e) = game.op_json(
        "input.mouse.click",
        &json!({"button": "left", "x": hx, "y": hy, "backend": "l1"}),
    ) {
        panic!("input.mouse.click failed: {e}");
    }

    let start = std::time::Instant::now();
    let deadline = start + timeout;
    while std::time::Instant::now() < deadline {
        let now_id = active_scene_id(game);
        if now_id != Some(baseline) {
            eprintln!(
                "[home-scene] active_scene_id transitioned {baseline} -> {now_id:?} \
                 in {:?}",
                start.elapsed()
            );
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    panic!(
        "ensure_home_scene_loaded: active_scene_id stayed at {baseline} for {timeout:?} \
         after click @ ({hx}, {hy}). Wrong coords, click didn't reach the game window, \
         or the gate needs a different signal."
    );
}

/// Block until `target_horse(game)` resolves successfully. Used as a
/// pre-test gate when the save may not be loaded yet.
pub fn wait_for_target_horse(game: &RunningGame, timeout: std::time::Duration) -> TargetHorse {
    let want = std::env::var("HORSEY_HORSE").ok();
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let horses = list_owned(game);
        if let Some(ref name) = want {
            if let Some(h) = find_owned(game, name) {
                eprintln!("found target horse '{}'", h.name);
                return h;
            }
        } else if !horses.is_empty() {
            return target_horse(game);
        }
        if std::time::Instant::now() >= deadline {
            let names: Vec<_> = horses.iter().filter_map(|h| {
                h.get("name").and_then(Value::as_str).map(String::from)
            }).collect();
            panic!("timed out waiting for target horse; want={want:?} owned={names:?}");
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}
