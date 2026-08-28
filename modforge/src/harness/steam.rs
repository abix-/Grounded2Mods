//! Steam launcher. Prefer `steam.exe -applaunch <id>` when the
//! exe is on disk; fall back to the `steam://rungameid/<id>`
//! protocol handler.

use std::path::PathBuf;
use std::process::Command;

const DEFAULT_STEAM_EXE: &str = r"C:\Games\Steam\steam.exe";

/// Resolve the Steam executable. Reads `MODFORGE_STEAM_EXE` env var
/// first, else uses `DEFAULT_STEAM_EXE`. Returns `Some(path)` only
/// if the file exists.
pub fn resolve_steam_exe() -> Option<PathBuf> {
    let candidate =
        std::env::var("MODFORGE_STEAM_EXE").unwrap_or_else(|_| DEFAULT_STEAM_EXE.to_string());
    let p = PathBuf::from(&candidate);
    if p.is_file() { Some(p) } else { None }
}

/// Launch a Steam app by AppID. Fires-and-forgets the Steam
/// process; the caller polls for the game process via
/// `process::wait_for_start`.
pub fn launch(app_id: u32) -> anyhow::Result<()> {
    if let Some(exe) = resolve_steam_exe() {
        Command::new(exe)
            .args(["-applaunch", &app_id.to_string()])
            .spawn()?;
        return Ok(());
    }
    // Fallback: protocol handler via `start steam://rungameid/<id>`.
    Command::new("cmd.exe")
        .args(["/c", "start", &format!("steam://rungameid/{app_id}")])
        .spawn()?;
    Ok(())
}
