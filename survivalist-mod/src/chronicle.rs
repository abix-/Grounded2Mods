//! The chronicle: the living world made visible IN the game
//! (docs/faction-war.md "Multidimensional factions").
//!
//! Everything the factions do to each other was only legible in
//! Player.log and the control-plane ops; the player saw none of
//! it. This module posts the dramatic beats to the game's own
//! status bar (`HudBehaviour.Instance.SetStatusBarMsg`, the same
//! surface the vanilla war banners use), phrased as word
//! traveling: "Word spreads: ...".
//!
//! Editorial line: only PUBLIC events post. A caught thief is a
//! scene; a clean getaway stays a secret. Vanilla's own banner
//! for AI-vs-AI hostility is misleading ("You are at war with X"
//! regardless of who declared on whom), so the mod's ignitions
//! pass showWarNotifications=false and post correct third-party
//! phrasing here instead.

use serde_json::json;

use unityforge::mono::MonoType;

/// Post one line to the in-game status bar. Best-effort: at the
/// menu (or if the HUD is not up) the word simply does not spread.
/// Stays here because it targets Survivalist's status-bar class and presentation behavior.
pub fn post(msg: &str) {
    let Some(hud) = MonoType::find("HudBehaviour").and_then(|t| t.singleton_instance()) else {
        return;
    };
    let _ = hud.invoke("SetStatusBarMsg", &json!([format!("Word spreads: {msg}")]));
}
