// User-editable settings. G2rpg-specific shape. Storage + atomic save
// lives in `ueforge::settings::Settings<T>`; consumers call
// `ueforge::settings::Settings::<Settings>::load("settings.json").get()`
// to read the file once at init.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub inventory: InventorySettings,
    #[serde(default)]
    pub survival: SurvivalSettings,
    #[serde(default)]
    pub rpg: RpgSettings,
    #[serde(default)]
    pub debug: DebugSettings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DebugSettings {
    /// HTTP debug endpoint port (`127.0.0.1:<port>`). Off (None) by
    /// default; opt in via `"debug": { "http_port": 17171 }` in
    /// settings.json.
    #[serde(default)]
    pub http_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySettings {
    #[serde(default = "default_slot_count")]
    pub slot_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurvivalSettings {
    #[serde(default = "default_survival_multiplier")]
    pub thirst_multiplier: f32,
    #[serde(default = "default_survival_multiplier")]
    pub hunger_multiplier: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpgSettings {
    /// Fraction of base XP awarded when the player's Buggy mount makes
    /// the killing blow. 1.0 = full XP. Set to 0.5 for half-credit, 0.0
    /// to ignore.
    #[serde(default = "default_buggy_kill_xp_multiplier")]
    pub buggy_kill_xp_multiplier: f32,
}

/// Supplies Grounded 2's default backpack size when the setting is omitted.
/// Stays here because the default is this mod's balance choice; Modforge owns settings storage.
fn default_slot_count() -> i32 {
    40
}
/// Leaves Grounded 2's hunger or thirst rate unchanged when the setting is omitted.
/// Stays here because the default is this mod's survival policy; Modforge owns settings storage.
fn default_survival_multiplier() -> f32 {
    1.0
}
/// Gives Buggy kills full XP when the setting is omitted.
/// Stays here because Buggy kill credit is a Grounded 2 rule; Modforge owns settings storage.
fn default_buggy_kill_xp_multiplier() -> f32 {
    1.0
}

impl Default for InventorySettings {
    /// Builds Grounded 2's default inventory settings.
    /// Stays here because this type contains this mod's backpack policy.
    fn default() -> Self {
        Self {
            slot_count: default_slot_count(),
        }
    }
}
impl Default for SurvivalSettings {
    /// Builds Grounded 2's default hunger and thirst settings.
    /// Stays here because this type contains this mod's survival policy.
    fn default() -> Self {
        Self {
            thirst_multiplier: default_survival_multiplier(),
            hunger_multiplier: default_survival_multiplier(),
        }
    }
}
impl Default for RpgSettings {
    /// Builds Grounded 2's default RPG settings.
    /// Stays here because this type contains this mod's Buggy XP policy.
    fn default() -> Self {
        Self {
            buggy_kill_xp_multiplier: default_buggy_kill_xp_multiplier(),
        }
    }
}

impl Settings {
    /// Writes the effective Grounded 2 settings to the mod log for player diagnosis.
    /// Stays here because the fields and summary are specific to this mod;
    /// Modforge owns settings loading; Ueforge owns the game log.
    pub fn log_summary(&self) {
        ueforge::log!(
            "settings: slot_count={}, thirst_mult={:.3}, hunger_mult={:.3}, buggy_xp_mult={:.3}, debug_http_port={:?}",
            self.inventory.slot_count,
            self.survival.thirst_multiplier,
            self.survival.hunger_multiplier,
            self.rpg.buggy_kill_xp_multiplier,
            self.debug.http_port
        );
    }
}
