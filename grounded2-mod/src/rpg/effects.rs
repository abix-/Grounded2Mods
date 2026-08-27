// Game-specific Effect implementations. The operations that
// don't fit the framework's standard Effect library.
//
// Per the workspace composition model
// (../../../ueforge/docs/architecture.md), each thing we research
// and figure out how to do in the game is one Effect type. This
// module owns the Maine-specific backpack operation:
//
//   - BackpackSlotsEffect     . Write inventory slot count to
//                                 every InventoryComponent CDO
//
// The survival-drain and fall-damage effect implementations are
// shared by Ueforge; this module supplies their Grounded 2 classes,
// offsets, settings, caches, and tuning.
//
// The framework's standard effects (PlayerFloat, SubcomponentMultiply,
// etc.) are used directly in `skills.rs` static declarations. No
// custom impl needed there.

use std::sync::OnceLock;

use ueforge::rpg::Effect;

use crate::inv_hook;
use crate::patch;
use crate::rpg::apply;
use crate::rpg::skills::{
    self, ASC_FALL_DAMAGE_RATIO, ASC_MINIMUM_FALL_DAMAGE_VELOCITY, ASC_TAKE_FALL_DAMAGE,
    GMS_FALL_DAMAGE_MULTIPLIER, SMMC_CUSTOM_FALL_DAMAGE_MULTIPLIER, SURVIVAL_HUNGER_OFFSET,
    SURVIVAL_THIRST_OFFSET,
};
use crate::rpg::world_loader;

// ---------------------------------------------------------------
// BackpackSlotsEffect. Adds skill-derived bonus slots to the
// settings-configured base inventory size, then patches every
// player InventoryComponent CDO + tells inv_hook the new count.
// ---------------------------------------------------------------

pub struct BackpackSlotsEffect {
    pub max_bonus_slots: i32,
}

impl Effect<ueforge::rpg::UeEngine> for BackpackSlotsEffect {
    /// Adds the purchased Backpack bonus to the configured inventory size and refreshes the UI hook.
    /// Stays here because Grounded 2 defines the backpack patch and inventory widget;
    /// Modforge owns generic skill-effect dispatch, and Ueforge owns Unreal access.
    fn apply(&self, level: u32, _max_level: u32, _ctx: &ueforge::rpg::TriggerCtx<'_>) {
        let Some(settings) = world_loader::loaded_settings() else {
            return;
        };
        let bonus = skills::backpack_bonus_at(level, self.max_bonus_slots);
        let target = settings.inventory.slot_count.saturating_add(bonus);
        let stats = patch::run(target);
        inv_hook::update_slot_count(target);
        ueforge::log!(
            "rpg/effects: backpack level={} target={} (base={} + bonus={}) patched={}",
            level,
            target,
            settings.inventory.slot_count,
            bonus,
            stats.patched
        );
    }

    /// Describes how many inventory slots the current Backpack level adds.
    /// Stays here because the slot bonus is Grounded 2 skill tuning;
    /// Modforge owns the effect display contract.
    fn format(&self, level: u32, _max_level: u32) -> String {
        let bonus = skills::backpack_bonus_at(level, self.max_bonus_slots);
        format!("+{bonus} slots")
    }
}

pub static BACKPACK: BackpackSlotsEffect = BackpackSlotsEffect {
    max_bonus_slots: 460,
};

// ---------------------------------------------------------------
// SurvivalDrainEffect configuration. Ueforge owns the calculation
// and CDO write; Grounded 2 owns every input.
// ---------------------------------------------------------------

fn hunger_settings_multiplier() -> Option<f32> {
    world_loader::loaded_settings().map(|settings| settings.survival.hunger_multiplier)
}

fn thirst_settings_multiplier() -> Option<f32> {
    world_loader::loaded_settings().map(|settings| settings.survival.thirst_multiplier)
}

pub static HUNGER_DRAIN: ueforge::rpg::SurvivalDrainEffect = ueforge::rpg::SurvivalDrainEffect {
    class: &apply::CLASS_SURVIVAL_COMPONENT,
    field_offset: SURVIVAL_HUNGER_OFFSET,
    vanilla: apply::vanilla_hunger,
    settings_multiplier: hunger_settings_multiplier,
    max_reduction: 0.75,
};

pub static THIRST_DRAIN: ueforge::rpg::SurvivalDrainEffect = ueforge::rpg::SurvivalDrainEffect {
    class: &apply::CLASS_SURVIVAL_COMPONENT,
    field_offset: SURVIVAL_THIRST_OFFSET,
    vanilla: apply::vanilla_thirst,
    settings_multiplier: thirst_settings_multiplier,
    max_reduction: 0.75,
};

// ---------------------------------------------------------------
// FallDamageReductionEffect configuration. Ueforge owns the
// composite write; Grounded 2 owns every class, offset, and value.
// ---------------------------------------------------------------

static VANILLA_FALL_DAMAGE_RATIO: OnceLock<f32> = OnceLock::new();
static VANILLA_MIN_FALL_DAMAGE_VELOCITY: OnceLock<f32> = OnceLock::new();
static VANILLA_GAME_MODE_FALL_DAMAGE_MULTIPLIER: OnceLock<f32> = OnceLock::new();
static VANILLA_SMMC_FALL_DAMAGE_MULTIPLIER: OnceLock<f32> = OnceLock::new();

pub static FALL_DAMAGE: ueforge::rpg::FallDamageReductionEffect =
    ueforge::rpg::FallDamageReductionEffect {
        player: &apply::PLAYER,
        ratio_offset: ASC_FALL_DAMAGE_RATIO,
        take_fall_damage_offset: ASC_TAKE_FALL_DAMAGE,
        min_velocity_offset: ASC_MINIMUM_FALL_DAMAGE_VELOCITY,
        game_mode_settings: &apply::CLASS_SURVIVAL_GAME_MODE_SETTINGS,
        game_mode_multiplier_offset: GMS_FALL_DAMAGE_MULTIPLIER,
        mode_manager_components: &apply::CLASS_SURVIVAL_MODE_MANAGER_COMPONENT,
        mode_manager_multiplier_offset: SMMC_CUSTOM_FALL_DAMAGE_MULTIPLIER,
        max_reduction: 1.00,
        min_velocity_multiplier_at_max: 100.0,
        disable_damage_at: 0.999,
        vanilla_ratio: &VANILLA_FALL_DAMAGE_RATIO,
        vanilla_min_velocity: &VANILLA_MIN_FALL_DAMAGE_VELOCITY,
        vanilla_game_mode_multiplier: &VANILLA_GAME_MODE_FALL_DAMAGE_MULTIPLIER,
        vanilla_mode_manager_multiplier: &VANILLA_SMMC_FALL_DAMAGE_MULTIPLIER,
    };

/// Returns the untouched Grounded 2 player fall-damage ratio captured by the skill effect.
/// Stays here because the baseline belongs to this game's composite fall-damage implementation.
pub(crate) fn vanilla_fall_damage_ratio() -> Option<f32> {
    VANILLA_FALL_DAMAGE_RATIO.get().copied()
}

/// Configures Ueforge's reusable outgoing-damage healing effect
/// for Grounded 2's player and health-component layout.
pub static LIFESTEAL: ueforge::rpg::LifestealEffect = ueforge::rpg::LifestealEffect {
    player: &apply::PLAYER,
    health_component_offset: apply::ASC_HEALTH_COMPONENT,
    current_damage_offset: 0x032C, // UHealthComponent.CurrentDamage
    max_fraction: 0.90,
};

/// Configures Ueforge's reusable post-damage reversal effect for
/// Grounded 2's environmental damage and health layout.
pub static IMPACT_REVERSAL: ueforge::rpg::ImpactReversalEffect =
    ueforge::rpg::ImpactReversalEffect {
        damage_info: crate::rpg::kill_hook::DAMAGE_INFO,
        current_damage_offset: 0x032C, // UHealthComponent.CurrentDamage
        damage_type_marker: "Environmental",
        max_reduction: 1.0,
    };
