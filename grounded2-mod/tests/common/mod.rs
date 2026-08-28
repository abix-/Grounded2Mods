// Shared client for integration tests against the mod's debug HTTP
// endpoint.
//
// Tests need a live, modded Grounded 2 instance. Set BBP_DEBUG_PORT
// to enable; if unset, tests skip with a clear message instead of
// failing CI. Default port matches `settings.example.json`.
//
// Run pattern (from a terminal, after launching Grounded 2 + loading a save):
//
//     set BBP_DEBUG_PORT=17171
//     cargo test --test bandage_regression. --test-threads=1 --nocapture
//
// Tests share a single global game-state, so always pass
// `--test-threads=1`. Otherwise concurrent ops will race.

#![allow(dead_code)]

use serde::Deserialize;
use serde_json::{Value, json};

/// Re-export of ueforge-client's perf-log writer. Called with
/// `env!("CARGO_MANIFEST_DIR")` evaluated in this crate so it
/// resolves to grounded2-mod's manifest, not ueforge-client's;
/// the writer walks up from there to find `.git`.
pub use ueforge::client::perf::PerfLog;

pub fn open_perf_log(test_name: &str) -> PerfLog {
    ueforge::client::perf::open(test_name, std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
}

// HTTP wrapper, hex codec, and parm helpers all live in
// `ueforge` (`client` feature). Re-export so test files keep using
// the same short names without churn.
#[allow(unused_imports)]
pub use hex::{decode as hex_decode, encode as hex_encode};
#[allow(unused_imports)]
pub use ueforge::parms::{as_bytes as parms_as_bytes, from_bytes as bytes_as_parms};

pub type OpResponse = ueforge::OpResponse<Snapshot>;

const ENV_PORT: &str = "BBP_DEBUG_PORT";

/// Test-side API wrapper. Newtype around `ueforge::client::Api<Snapshot>`
/// so we can hang Grounded2-specific convenience helpers
/// (`skill_spend`, `skill_toggle`, ...) on it.
pub struct Api(ueforge::client::Api<Snapshot>);

impl Api {
    /// Connect to the running mod. If `BBP_DEBUG_PORT` is unset
    /// or unparseable, returns `None` so tests can skip cleanly.
    pub fn try_connect() -> Option<Self> {
        ueforge::client::Api::try_connect(ENV_PORT, "/debug").map(Self)
    }

    /// Connect or panic. Use when the test absolutely requires the
    /// endpoint.
    pub fn require() -> Self {
        Self(ueforge::client::Api::require(ENV_PORT, "/debug"))
    }

    pub fn op(&self, op: &str, args: Value) -> OpResponse {
        self.0.op(op, args)
    }

    /// Borrow the underlying `ueforge::client::Api`. Used by
    /// generic helpers in `ueforge::client` that need
    /// the raw API.
    pub fn inner(&self) -> &ueforge::client::Api<Snapshot> {
        &self.0
    }

    pub fn op_ok(&self, op: &str, args: Value) -> Snapshot {
        self.0.op_ok(op, args)
    }

    pub fn snapshot(&self) -> Snapshot {
        self.0.snapshot()
    }

    /// Generic UFunction call (selectors: `live_player_hc`,
    /// `live_player`, `first_class:<Name>`, `addr:0x<hex>`).
    pub fn call_ufunction(
        &self,
        class: &str,
        function: &str,
        instance_selector: &str,
        parms_bytes: &[u8],
    ) -> Result<(Vec<u8>, Snapshot), String> {
        self.0
            .call_ufunction(class, function, instance_selector, parms_bytes)
    }

    // ---- Grounded2-specific convenience helpers ----

    pub fn skill_spend(&self, id: &str, count: u32) -> Snapshot {
        self.op_ok("skill_spend", json!({"id": id, "count": count}))
    }

    pub fn skill_refund(&self, id: &str, count: u32) -> Snapshot {
        self.op_ok("skill_refund", json!({"id": id, "count": count}))
    }

    pub fn skill_toggle(&self, id: &str, enabled: bool) -> Snapshot {
        self.op_ok("skill_toggle", json!({"id": id, "enabled": enabled}))
    }

    pub fn simulate_heal(&self, amount: f32) -> Snapshot {
        self.op_ok("simulate_heal", json!({"amount": amount}))
    }
}

impl Snapshot {
    pub fn skill_level(&self, id: &str) -> u32 {
        self.player_state
            .as_ref()
            .and_then(|ps| ps.skill_levels.get(id))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32
    }

    pub fn is_disabled(&self, id: &str) -> bool {
        self.disabled_skills.iter().any(|s| s == id)
    }

    pub fn live_hc(&self) -> &HcFields {
        self.live_player
            .as_ref()
            .expect("no live player")
            .hc
            .as_ref()
            .expect("no live HC")
    }

    pub fn hp(&self) -> f32 {
        let hc = self.live_hc();
        // CurrentDamage on UHealthComponent is "damage taken so far",
        // so HP = MaxHealth - CurrentDamage.
        hc.max_health - hc.current_damage
    }
}

// `OpResponse` is `ueforge::OpResponse<Snapshot>` (aliased
// near the top of this file).

#[derive(Debug, Deserialize)]
pub struct Snapshot {
    pub slot: Option<String>,
    pub player_state: Option<PlayerStateView>,
    #[serde(default)]
    pub disabled_skills: Vec<String>,
    pub vanilla: VanillaView,
    pub live_player: Option<LivePlayerView>,
    pub cdo: CdoView,
    pub inventory: InventoryView,
    #[serde(default)]
    pub status_effects: Option<Vec<StatusEffectView>>,
    #[serde(default)]
    pub catalog: Vec<CatalogEntry>,
    pub settings: SettingsView,
    #[serde(default)]
    pub damage_ring: Vec<DamageEvent>,
    #[serde(default)]
    pub counters: Value,
    #[serde(default)]
    pub process_memory: Value,
    #[serde(default)]
    pub process_cpu: Value,
    #[serde(default)]
    pub process_threads: Value,
    #[serde(default)]
    pub game_population: Value,
    #[serde(default)]
    pub process_regions: Value,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DamageEvent {
    pub at_secs: u64,
    pub function: String,
    pub damage: f32,
    pub damage_flags: i32,
    pub type_flags: u32,
    pub current_damage_before: Option<f32>,
    pub current_damage_after: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub struct StatusEffectView {
    pub row: String,
    pub table: String,
    pub stat_type: Option<u8>,
    pub value: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub display_name: String,
    pub max_level: u32,
    pub effect_kind: String,
}

#[derive(Debug, Deserialize)]
pub struct SettingsView {
    pub inventory_slot_count: i32,
    pub thirst_multiplier: f32,
    pub hunger_multiplier: f32,
    pub buggy_kill_xp_multiplier: f32,
    pub debug_http_port: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub struct InventoryView {
    pub current_slot_count: i32,
}

#[derive(Debug, Deserialize)]
pub struct PlayerStateView {
    pub xp: u64,
    pub level: u32,
    pub skill_points: u32,
    #[serde(default)]
    pub skill_levels: serde_json::Map<String, Value>,
    pub kill_count: u64,
    pub last_killed: String,
}

#[derive(Debug, Deserialize)]
pub struct VanillaView {
    pub hunger: Option<f32>,
    pub thirst: Option<f32>,
    pub fall_damage_ratio: Option<f32>,
    pub max_health: Option<f32>,
    #[serde(default)]
    pub hc_u32_masks: Vec<HcMaskView>,
}

#[derive(Debug, Deserialize)]
pub struct HcMaskView {
    pub offset: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct LivePlayerView {
    pub instance_count: usize,
    pub hc: Option<HcFields>,
    pub cmc: Option<CmcFields>,
    pub asc: Option<AscFields>,
}

#[derive(Debug, Deserialize)]
pub struct HcFields {
    pub required_damage_type_flags: String,
    pub immunity_flags: String,
    pub base_damage_reduction: f32,
    pub max_health: f32,
    pub current_damage: f32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CmcFields {
    pub max_walk_speed: f32,
    pub max_sprint_speed: f32,
    pub max_swim_speed: f32,
    pub max_swim_sprint_speed: f32,
    pub max_fly_speed: f32,
    pub jump_z_velocity: f32,
    pub air_control: f32,
    pub air_control_boost_multiplier: f32,
    pub air_control_boost_velocity_threshold: f32,
    pub custom_ground_speed_multiplier: f32,
    pub custom_fly_speed_multiplier: f32,
    pub custom_swim_speed_multiplier: f32,
}

#[derive(Debug, Deserialize)]
pub struct AscFields {
    pub custom_damage_multiplier: f32,
    pub take_fall_damage: bool,
    pub minimum_fall_damage_velocity: f32,
    pub fall_damage_ratio: f32,
}

#[derive(Debug, Deserialize)]
pub struct CdoView {
    pub survival: Option<SurvivalCdoFields>,
    pub global_combat_data: Option<GlobalCombatDataFields>,
    pub game_mode_settings: Option<GmsFields>,
    pub survival_mode_manager: Option<SmmcFields>,
}

#[derive(Debug, Deserialize)]
pub struct SurvivalCdoFields {
    pub hunger_adjustment_per_second: f32,
    pub thirst_adjustment_per_second: f32,
}

#[derive(Debug, Deserialize)]
pub struct GlobalCombatDataFields {
    pub combat_regen_delay: f32,
    pub combat_regen_tick_percentage: f32,
    pub combat_regen_tick_rate: f32,
}

#[derive(Debug, Deserialize)]
pub struct GmsFields {
    pub fall_damage_multiplier: f32,
}

#[derive(Debug, Deserialize)]
pub struct SmmcFields {
    pub custom_fall_damage_multiplier: f32,
}
