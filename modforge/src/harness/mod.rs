//! Generic Steam-game test harness.
//!
//! Drives the full repeatable loop: build the mod, kill any running
//! game instance, launch via Steam, optionally inject a DLL, poll the
//! mod's HTTP endpoint until ready, hand back a `RunningGame` handle
//! that exposes the HTTP API. On `Drop` or explicit `stop()` the
//! game process is taskkilled.
//!
//! Per-game tests wire this up via a `GameDef` constant and a
//! `tests/common/mod.rs` helper. See `horsey-mod/tests/common/mod.rs`
//! for the reference consumer.
//!
//! Env vars honored:
//! - `MODFORGE_NO_GAME=1`: tests should skip with a clear message
//!   instead of attempting to launch. Helper: [`should_skip`].
//! - `MODFORGE_STEAM_EXE`: override Steam executable path. Default:
//!   `C:\Games\Steam\steam.exe` falling back to the
//!   `steam://rungameid/<app_id>` protocol handler.
//! - `MODFORGE_SKIP_BUILD=1`: assume the mod is already built; skip
//!   the `cargo build` step. Useful when iterating manually.

pub mod build;
pub mod http_probe;
pub mod injector;
pub mod log;
pub mod process;
pub mod steam;

use std::path::PathBuf;
use std::time::Duration;

/// Description of one moddable Steam game and the HTTP plane its
/// mod exposes. Construct once per game in `tests/common/mod.rs`.
#[derive(Debug, Clone)]
pub struct GameDef {
    /// Steam AppID. Used for `steam -applaunch <id>` and the
    /// `steam://rungameid/<id>` fallback.
    pub app_id: u32,
    /// Process name as it shows up in `tasklist` (e.g. `Horsey.exe`).
    /// Used to detect the running instance and to kill it.
    pub process_name: &'static str,
    /// HTTP plane probe: port, endpoint, optional auth header.
    pub http: HttpProbe,
    /// Optional injector: present for native-PE games where the mod
    /// is a DLL loaded post-launch. UE / Unity games that load their
    /// mod at startup leave this `None`.
    pub injector: Option<InjectorDef>,
    /// How long to wait between (a) launching the game and (b) the
    /// HTTP endpoint responding ok. Default 120s.
    pub launch_timeout: Duration,
    /// Grace period from `stop()` before taskkill. Default 3s.
    pub shutdown_grace: Duration,
    /// Optional `cargo build -p <name>` to run before launching.
    /// `None` means skip the build step entirely.
    pub build: Option<BuildDef>,
}

impl GameDef {
    pub fn defaults(
        app_id: u32,
        process_name: &'static str,
        http: HttpProbe,
    ) -> Self {
        Self {
            app_id,
            process_name,
            http,
            injector: None,
            launch_timeout: Duration::from_secs(120),
            shutdown_grace: Duration::from_secs(3),
            build: None,
        }
    }
}

/// HTTP plane probe. The mod's `modforge::server::spawn` registers
/// one endpoint that takes POSTed op requests; we hit `op/<name>` at
/// that endpoint.
#[derive(Debug, Clone)]
pub struct HttpProbe {
    /// Port the mod listens on. Horsey-mod default: 33077.
    pub port: u16,
    /// URL path the mod accepts POSTs at. Horsey-mod default: `/op`.
    pub endpoint: &'static str,
    /// Optional auth header to send with every request. `None`
    /// means auth is disabled (DLL writes the token to a file
    /// next to the binary but in test mode we usually disable
    /// auth entirely).
    pub auth_token: Option<String>,
}

impl HttpProbe {
    pub fn new(port: u16, endpoint: &'static str) -> Self {
        Self {
            port,
            endpoint,
            auth_token: None,
        }
    }
}

/// Injector specification. For native-PE games like Horsey we run
/// a separate `*-inject.exe` after the game starts to load the
/// mod DLL via `CreateRemoteThread(LoadLibraryW)`.
#[derive(Debug, Clone)]
pub struct InjectorDef {
    /// Path to the injector binary. Typically built alongside the
    /// mod DLL by the same workspace.
    pub injector_exe: PathBuf,
    /// Extra CLI args to pass to the injector. The default injector
    /// resolves the target process by `game_name_regex` in its own
    /// metadata, so this is usually empty.
    pub args: Vec<String>,
    /// How long to wait after `injector_exe` exits before probing
    /// the HTTP endpoint. Some DLLs spawn worker threads that take
    /// a moment to bind the port. Default 500ms.
    pub post_inject_settle: Duration,
}

impl InjectorDef {
    pub fn new(injector_exe: PathBuf) -> Self {
        Self {
            injector_exe,
            args: Vec::new(),
            post_inject_settle: Duration::from_millis(500),
        }
    }
}

/// Build step. Runs `cargo build -p <package>` (optionally
/// `--release`) before launching the game so the injected DLL
/// matches the test code.
#[derive(Debug, Clone)]
pub struct BuildDef {
    /// Workspace package name passed as `cargo build -p <name>`.
    pub package: &'static str,
    /// If true, `cargo build --release`. Default false.
    pub release: bool,
    /// Workspace root. Used as cwd for `cargo build`.
    pub workspace_dir: PathBuf,
}

/// Should the current test skip game launch? Reads
/// `MODFORGE_NO_GAME` from the environment. Tests should call this
/// at the top and return cleanly if it returns true.
pub fn should_skip() -> bool {
    matches!(std::env::var("MODFORGE_NO_GAME").as_deref(), Ok("1") | Ok("true"))
}

/// Should the build step be skipped? Reads `MODFORGE_SKIP_BUILD`.
pub fn should_skip_build() -> bool {
    matches!(
        std::env::var("MODFORGE_SKIP_BUILD").as_deref(),
        Ok("1") | Ok("true")
    )
}

mod harness;
pub use harness::{GameHarness, RunningGame};
