// Skill catalog. Per the workspace composition model
// (../../../ueforge/docs/architecture.md):
//
//   - Each operation we figured out how to perform is an Effect.
//     Framework effects (PlayerFloat, SubcomponentMultiply, ...) are
//     declared as `static`s here, one per skill. Game-specific
//     effects (BackpackSlots, SurvivalDrain, fall-damage composite)
//     live in `effects.rs`.
//   - A Skill is one Effect applied with parameters at a level.
//     The `CATALOG` rows below pair an id + display name + max level
//     with an EffectDef referencing one of the static instances.
//
// Adding a new skill:
//   1. If the operation already exists, declare a `static` of that
//      effect type with the parameters you want.
//   2. If the operation is new, add an Effect impl + static in
//      effects.rs (or contribute it to ueforge if other games would
//      want it).
//   3. Add a CATALOG row referencing the new static.
//
// No central match arm to update.

// ---------------------------------------------------------------------
// Skill IDs (stable strings used as keys in PlayerState.skill_levels
// and in the FFI surface).
// ---------------------------------------------------------------------

pub const SKILL_BACKPACK: &str = "backpack";
pub const SKILL_HUNGER: &str = "hunger";
pub const SKILL_THIRST: &str = "thirst";
pub const SKILL_ATTACK_DAMAGE: &str = "attack_damage";
pub const SKILL_ARMOR: &str = "armor";
pub const SKILL_MOVE_SPEED: &str = "move_speed";
pub const SKILL_JUMP_HEIGHT: &str = "jump_height";
pub const SKILL_GLIDE_SPEED: &str = "glide_speed";
pub const SKILL_FALL_RESISTANCE: &str = "fall_resistance";
pub const SKILL_IMPACT_RESISTANCE: &str = "impact_resistance";
pub const SKILL_LIFESTEAL: &str = "lifesteal";
pub const SKILL_MAX_HEALTH: &str = "max_health";
pub const SKILL_LEAP_DISTANCE: &str = "leap_distance";
pub const SKILL_HEALTH_REGEN: &str = "health_regen";

/// Universal cap. Every skill scales 0..=100.
pub const SKILL_MAX_LEVEL: u32 = 100;

pub use ueforge::rpg::PercentFormat;

// ---------------------------------------------------------------------
// Field offsets used by the catalog. Centralized here so the catalog
// rows stay readable.
//
// Source: C:\tools\work\sdk\SDK\ (Dumper-7 SDK), citations follow each
// constant.
// ---------------------------------------------------------------------

// SurvivalComponent.HungerSettings.AdjustmentPerSecond at +0x140.
pub const SURVIVAL_HUNGER_OFFSET: usize = 0x0140;
// SurvivalComponent.ThirstSettings.AdjustmentPerSecond at +0x188.
pub const SURVIVAL_THIRST_OFFSET: usize = 0x0188;

// ASurvivalCharacter.CustomDamageMultiplier (Maine_classes.hpp:5771).
pub const ASC_CUSTOM_DAMAGE_MULTIPLIER: usize = 0x12B8;
// ASurvivalCharacter.bTakeFallDamage (Maine_classes.hpp:5846).
pub const ASC_TAKE_FALL_DAMAGE: usize = 0x1571;
// ASurvivalCharacter.MinimumFallDamageVelocity (Maine_classes.hpp:5848).
pub const ASC_MINIMUM_FALL_DAMAGE_VELOCITY: usize = 0x1574;
// ASurvivalCharacter.FallDamageRatio (Maine_classes.hpp:5850).
pub const ASC_FALL_DAMAGE_RATIO: usize = 0x157C;

// USurvivalGameModeSettings.FallDamageMultiplier (Maine_classes.hpp:36808).
pub const GMS_FALL_DAMAGE_MULTIPLIER: usize = 0x008C;

// USurvivalModeManagerComponent.CustomSettings (Maine_classes.hpp:37142):
pub const SMMC_CUSTOM_SETTINGS_OFFSET: usize = 0x0114;
pub const SMMC_CUSTOM_FALL_DAMAGE_MULTIPLIER: usize =
    SMMC_CUSTOM_SETTINGS_OFFSET + FCG_FALL_DAMAGE_MULTIPLIER_OFFSET;

// FCustomGameModeSettings struct layout (Maine_structs.hpp:6959, 0x20 bytes).
pub const FCG_FALL_DAMAGE_MULTIPLIER_OFFSET: usize = 0x001C;
pub const FCG_STRUCT_SIZE: usize = 0x0020;

// UHealthComponent.RequiredDamageTypeFlags (Maine_classes.hpp:42197).
pub const HC_REQUIRED_DAMAGE_TYPE_FLAGS: usize = 0x00FC;

// UHealthComponent.BaseDamageReduction (Maine_classes.hpp:42193).
pub const HC_BASE_DAMAGE_REDUCTION: usize = 0x00EC;

// UGlobalCombatData (Maine_classes.hpp:23883).
pub const GCD_COMBAT_REGEN_DELAY: usize = 0x0108;
pub const GCD_COMBAT_REGEN_TICK_PERCENTAGE: usize = 0x010C;
pub const GCD_COMBAT_REGEN_TICK_RATE: usize = 0x0110;

// UHealthComponent.MaxHealth (../docs/damage.md "HealthComponent layout").
pub const HC_MAX_HEALTH: usize = 0x0328;

// UCharacterMovementComponent.* (Engine_classes.hpp).
pub const CMC_JUMP_Z_VELOCITY: usize = 0x01B8;
pub const CMC_AIR_CONTROL: usize = 0x02C0;
pub const CMC_AIR_CONTROL_BOOST_MULT: usize = 0x02C4;
pub const CMC_AIR_CONTROL_BOOST_THRESHOLD: usize = 0x02C8;
pub const CMC_MAX_WALK_SPEED: usize = 0x0288;
pub const CMC_MAX_SWIM_SPEED: usize = 0x0290;
pub const CMC_MAX_FLY_SPEED: usize = 0x0294;
pub const CMC_MAX_SPRINT_SPEED: usize = 0x10EC;
pub const CMC_MAX_SWIM_SPRINT_SPEED: usize = 0x1164;
pub const CMC_CUSTOM_GROUND_SPEED_MULTIPLIER: usize = 0x1198;
pub const CMC_CUSTOM_FLY_SPEED_MULTIPLIER: usize = 0x119C;
pub const CMC_CUSTOM_SWIM_SPEED_MULTIPLIER: usize = 0x11A0;

// Movement axis groupings.
const MOVE_SPEED_OFFSETS: &[usize] = &[
    CMC_MAX_WALK_SPEED,
    CMC_MAX_SPRINT_SPEED,
    CMC_MAX_SWIM_SPEED,
    CMC_MAX_SWIM_SPRINT_SPEED,
    CMC_CUSTOM_GROUND_SPEED_MULTIPLIER,
    CMC_CUSTOM_SWIM_SPEED_MULTIPLIER,
];
const JUMP_HEIGHT_OFFSETS: &[usize] = &[CMC_JUMP_Z_VELOCITY];
const GLIDE_SPEED_OFFSETS: &[usize] = &[CMC_MAX_FLY_SPEED, CMC_CUSTOM_FLY_SPEED_MULTIPLIER];
const HEALTH_REGEN_OFFSETS: &[(usize, f32)] = &[
    (GCD_COMBAT_REGEN_TICK_PERCENTAGE, 1.0),
    (GCD_COMBAT_REGEN_TICK_RATE, -1.0),
];
const LEAP_DISTANCE_OFFSETS: &[usize] = &[
    CMC_AIR_CONTROL,
    CMC_AIR_CONTROL_BOOST_MULT,
    CMC_AIR_CONTROL_BOOST_THRESHOLD,
];

// ---------------------------------------------------------------------
// Static Effect instances. One per catalog row that uses a framework
// effect type. Game-specific effects live in `effects.rs`.
// ---------------------------------------------------------------------

use ueforge::rpg::{
    ClassFieldsMultiplyEffect, EffectDef, PlayerFloatEffect, SkillDef,
    SkillRegistry, SubcomponentAdditiveEffect, SubcomponentFloatEffect,
    SubcomponentMultiplyEffect,
};
use ueforge::rpg::trigger::{ON_DAMAGE_DEALT, ON_DAMAGE_TAKEN, ON_SLOT_CHANGE};
use ueforge::ue::TypedField;

static EFFECT_ATTACK_DAMAGE: PlayerFloatEffect = PlayerFloatEffect {
    player: &crate::rpg::apply::PLAYER,
    offset: TypedField::at(ASC_CUSTOM_DAMAGE_MULTIPLIER),
    base: 1.0,
    max_bonus: 3.00,
    format: PercentFormat::PlusPercentMult { word: "damage" },
};

static EFFECT_ARMOR: SubcomponentFloatEffect = SubcomponentFloatEffect {
    player: &crate::rpg::apply::PLAYER,
    component_offset: TypedField::at(crate::rpg::apply::ASC_HEALTH_COMPONENT),
    field_offset: TypedField::at(HC_BASE_DAMAGE_REDUCTION),
    base: 0.0,
    max_bonus: 0.50,
    format: PercentFormat::MinusPercent { word: "damage taken" },
};

static EFFECT_MOVE_SPEED: SubcomponentMultiplyEffect = SubcomponentMultiplyEffect {
    player: &crate::rpg::apply::PLAYER,
    component_offset: TypedField::at(crate::rpg::apply::ASC_CHAR_MOVEMENT_COMPONENT),
    offsets: MOVE_SPEED_OFFSETS,
    max_bonus: 3.00,
    format_word: "move",
    vanilla: &crate::rpg::apply::MOVEMENT_VANILLA,
};

static EFFECT_JUMP_HEIGHT: SubcomponentMultiplyEffect = SubcomponentMultiplyEffect {
    player: &crate::rpg::apply::PLAYER,
    component_offset: TypedField::at(crate::rpg::apply::ASC_CHAR_MOVEMENT_COMPONENT),
    offsets: JUMP_HEIGHT_OFFSETS,
    max_bonus: 3.00,
    format_word: "jump",
    vanilla: &crate::rpg::apply::MOVEMENT_VANILLA,
};

static EFFECT_LEAP_DISTANCE: SubcomponentMultiplyEffect = SubcomponentMultiplyEffect {
    player: &crate::rpg::apply::PLAYER,
    component_offset: TypedField::at(crate::rpg::apply::ASC_CHAR_MOVEMENT_COMPONENT),
    offsets: LEAP_DISTANCE_OFFSETS,
    max_bonus: 5.00,
    format_word: "leap",
    vanilla: &crate::rpg::apply::MOVEMENT_VANILLA,
};

static EFFECT_GLIDE_SPEED: SubcomponentMultiplyEffect = SubcomponentMultiplyEffect {
    player: &crate::rpg::apply::PLAYER,
    component_offset: TypedField::at(crate::rpg::apply::ASC_CHAR_MOVEMENT_COMPONENT),
    offsets: GLIDE_SPEED_OFFSETS,
    max_bonus: 3.00,
    format_word: "glide",
    vanilla: &crate::rpg::apply::MOVEMENT_VANILLA,
};

static EFFECT_MAX_HEALTH: SubcomponentAdditiveEffect = SubcomponentAdditiveEffect {
    player: &crate::rpg::apply::PLAYER,
    component_offset: TypedField::at(crate::rpg::apply::ASC_HEALTH_COMPONENT),
    field_offset: TypedField::at(HC_MAX_HEALTH),
    max_bonus: 200.0,
    format_word: "max HP",
    vanilla: &crate::rpg::apply::MAX_HEALTH_VANILLA,
};

static EFFECT_HEALTH_REGEN: ClassFieldsMultiplyEffect = ClassFieldsMultiplyEffect {
    class: &crate::rpg::apply::CLASS_GLOBAL_COMBAT_DATA,
    offsets: HEALTH_REGEN_OFFSETS,
    max_bonus: 5.00,
    format_word: "regen",
    vanilla: &crate::rpg::apply::GLOBAL_DATA_VANILLA,
};

// ---------------------------------------------------------------------
// CATALOG: skill rows compose Effect instances with parameters.
// ---------------------------------------------------------------------

pub const CATALOG_ENTRIES: &[SkillDef] = &[
    SkillDef {
        id: SKILL_BACKPACK,
        display_name: "Backpack",
        max_level: SKILL_MAX_LEVEL,
        effect: EffectDef::new("BackpackSlots", &crate::rpg::effects::BACKPACK),
        trigger: &ON_SLOT_CHANGE,
    },
    SkillDef {
        id: SKILL_HUNGER,
        display_name: "Hunger Resistance",
        max_level: SKILL_MAX_LEVEL,
        effect: EffectDef::new("SurvivalDrain", &crate::rpg::effects::HUNGER_DRAIN),
        trigger: &ON_SLOT_CHANGE,
    },
    SkillDef {
        id: SKILL_THIRST,
        display_name: "Thirst Resistance",
        max_level: SKILL_MAX_LEVEL,
        effect: EffectDef::new("SurvivalDrain", &crate::rpg::effects::THIRST_DRAIN),
        trigger: &ON_SLOT_CHANGE,
    },
    SkillDef {
        id: SKILL_ATTACK_DAMAGE,
        display_name: "Attack Damage",
        max_level: SKILL_MAX_LEVEL,
        effect: EffectDef::new("PlayerFloat", &EFFECT_ATTACK_DAMAGE),
        trigger: &ON_SLOT_CHANGE,
    },
    SkillDef {
        id: SKILL_ARMOR,
        display_name: "Armor",
        max_level: SKILL_MAX_LEVEL,
        effect: EffectDef::new("SubcomponentFloat", &EFFECT_ARMOR),
        trigger: &ON_SLOT_CHANGE,
    },
    SkillDef {
        id: SKILL_MOVE_SPEED,
        display_name: "Move Speed",
        max_level: SKILL_MAX_LEVEL,
        effect: EffectDef::new("SubcomponentMultiply", &EFFECT_MOVE_SPEED),
        trigger: &ON_SLOT_CHANGE,
    },
    SkillDef {
        id: SKILL_JUMP_HEIGHT,
        display_name: "Jump Height",
        max_level: SKILL_MAX_LEVEL,
        effect: EffectDef::new("SubcomponentMultiply", &EFFECT_JUMP_HEIGHT),
        trigger: &ON_SLOT_CHANGE,
    },
    SkillDef {
        id: SKILL_LEAP_DISTANCE,
        display_name: "Leap Distance",
        max_level: SKILL_MAX_LEVEL,
        effect: EffectDef::new("SubcomponentMultiply", &EFFECT_LEAP_DISTANCE),
        trigger: &ON_SLOT_CHANGE,
    },
    SkillDef {
        id: SKILL_GLIDE_SPEED,
        display_name: "Glide Speed",
        max_level: SKILL_MAX_LEVEL,
        effect: EffectDef::new("SubcomponentMultiply", &EFFECT_GLIDE_SPEED),
        trigger: &ON_SLOT_CHANGE,
    },
    SkillDef {
        id: SKILL_FALL_RESISTANCE,
        display_name: "Fall Damage Resistance",
        max_level: SKILL_MAX_LEVEL,
        effect: EffectDef::new(
            "PlayerFallDamageReduction",
            &crate::rpg::effects::FALL_DAMAGE,
        ),
        trigger: &ON_SLOT_CHANGE,
    },
    SkillDef {
        id: SKILL_IMPACT_RESISTANCE,
        display_name: "Impact Damage Resistance",
        max_level: SKILL_MAX_LEVEL,
        effect: EffectDef::new(
            "ImpactReversal",
            &crate::rpg::effects::IMPACT_REVERSAL,
        ),
        trigger: &ON_DAMAGE_TAKEN,
    },
    SkillDef {
        id: SKILL_MAX_HEALTH,
        display_name: "Max Health",
        max_level: SKILL_MAX_LEVEL,
        effect: EffectDef::new("SubcomponentAdditive", &EFFECT_MAX_HEALTH),
        trigger: &ON_SLOT_CHANGE,
    },
    SkillDef {
        id: SKILL_HEALTH_REGEN,
        display_name: "Health Regen",
        max_level: SKILL_MAX_LEVEL,
        effect: EffectDef::new("ClassFieldsMultiply", &EFFECT_HEALTH_REGEN),
        trigger: &ON_SLOT_CHANGE,
    },
    SkillDef {
        id: SKILL_LIFESTEAL,
        display_name: "Lifesteal",
        max_level: SKILL_MAX_LEVEL,
        effect: EffectDef::new(
            "Lifesteal",
            &crate::rpg::effects::LIFESTEAL,
        ),
        trigger: &ON_DAMAGE_DEALT,
    },
];

/// Canonical catalog handle.
pub static CATALOG: SkillRegistry = SkillRegistry::new(CATALOG_ENTRIES);

/// Finds a Grounded 2 skill definition by its stable identifier.
/// Stays here because this mod owns the skill catalog; Modforge owns the reusable registry lookup.
pub fn lookup(id: &str) -> Option<&'static SkillDef> {
    CATALOG.def(id)
}

// ---------------------------------------------------------------------
// Per-level math.
// ---------------------------------------------------------------------

/// Converts a Grounded 2 skill level into its curved zero-to-one progress.
/// Stays here because it binds Modforge's reusable curve to this mod's universal skill cap.
pub fn level_progress(level: u32) -> f32 {
    ueforge::rpg::progress::sqrt_progress(level, SKILL_MAX_LEVEL)
}

/// Scales a Grounded 2 skill's maximum bonus by the current level.
/// Stays here because this convenience binds shared progression math to this mod's skill cap.
pub fn skill_bonus(max_bonus: f32, level: u32) -> f32 {
    max_bonus * level_progress(level)
}

/// Calculates how many backpack slots the purchased skill level adds.
/// Stays here because slot rounding and the maximum bonus are Grounded 2 skill behavior;
/// Modforge owns the underlying progression curve.
pub fn backpack_bonus_at(level: u32, max_bonus_slots: i32) -> i32 {
    (max_bonus_slots as f32 * level_progress(level)).round() as i32
}

/// Calculates a runtime fraction from a Grounded 2 skill level and configured maximum.
/// Stays here because callers use this mod's skill cap; Modforge owns the progression curve.
pub fn runtime_fraction(level: u32, max_bonus: f32) -> f32 {
    skill_bonus(max_bonus, level)
}

/// Format effect text for the ImGui tab. Now a thin wrapper that
/// delegates to the EffectDef on the catalog row.
/// Stays here because it resolves Grounded 2's catalog entries;
/// Modforge owns each effect's reusable formatting contract.
pub fn format_effect(id: &str, level: u32) -> String {
    let Some(skill) = lookup(id) else {
        return String::new();
    };
    skill.effect.format(level, skill.max_level)
}
