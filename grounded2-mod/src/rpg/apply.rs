// Apply step: shared helpers + class refs + vanilla caches that
// the per-skill Effect impls (in `effects.rs`) consume. The
// game-side dispatch match this file used to own is GONE. Per
// the workspace composition model
// (../../../ueforge/docs/architecture.md), each catalog row's
// EffectDef is its own apply path. Tracker iterates the catalog
// and calls `effect.apply(level, max)` directly.
//
// What's left here:
//   - PLAYER, CLASS_GLOBAL_COMBAT_DATA. Shared class refs.
//   - capture_vanilla(). Runs at worker init, snapshots
//     hunger/thirst drain rates from the SurvivalComponent CDO.
//   - vanilla_hunger / vanilla_thirst accessors.
//   - DISABLED_SKILLS toggle set surface (still used by some
//     game-side code; the framework Tracker has its own copy).
//   - VanillaCaches used by static Effect instances.
//   - Walker helpers for the game-specific Effect impls.

use std::sync::OnceLock;

// Re-export field helpers under crate::rpg::apply so debug.rs +
// effects.rs can pull them from one place.
pub(crate) use ueforge::ue::field::{read_component_ptr, read_f32, read_u32};
use ueforge::ue::{self, GObjectsView, UObject};

use crate::survival;

// ASurvivalCharacter.HealthComponent ptr (Maine_classes.hpp:5782).
pub(crate) const ASC_HEALTH_COMPONENT: usize = 0x1340;
// ASurvivalCharacter.CharMovementComponent ptr (Maine_classes.hpp:5790).
pub(crate) const ASC_CHAR_MOVEMENT_COMPONENT: usize = 0x1380;

static VANILLA_HUNGER: OnceLock<f32> = OnceLock::new();
static VANILLA_THIRST: OnceLock<f32> = OnceLock::new();
static VANILLA_MAX_HEALTH: OnceLock<f32> = OnceLock::new();

/// Returns the untouched Grounded 2 hunger drain captured before skills are applied.
/// Stays here because the cache is populated from this game's survival component;
/// Modforge owns the reusable vanilla-cache primitive.
pub(crate) fn vanilla_hunger() -> Option<f32> {
    VANILLA_HUNGER.get().copied()
}
/// Returns the untouched Grounded 2 thirst drain captured before skills are applied.
/// Stays here because the cache is populated from this game's survival component;
/// Modforge owns the reusable vanilla-cache primitive.
pub(crate) fn vanilla_thirst() -> Option<f32> {
    VANILLA_THIRST.get().copied()
}
/// Returns the untouched Grounded 2 fall-damage ratio captured by its composite effect.
/// Stays here because it exposes this game's field baseline to the Grounded 2 debug snapshot.
pub(crate) fn vanilla_fall_damage_ratio() -> Option<f32> {
    crate::rpg::effects::vanilla_fall_damage_ratio()
}
/// Returns the untouched Grounded 2 maximum health captured before skill changes.
/// Stays here because it exposes this game's health baseline to the Grounded 2 debug snapshot.
pub(crate) fn vanilla_max_health() -> Option<f32> {
    VANILLA_MAX_HEALTH.get().copied()
}

/// Snapshot every captured `(offset, value)` pair we have for the
/// HC u32 mask family. Used by the debug snapshot endpoint.
/// Stays here because the offsets describe Grounded 2's health component;
/// Modforge owns the reusable keyed vanilla cache.
pub(crate) fn vanilla_hc_masks_snapshot() -> Vec<(usize, u32)> {
    VANILLA_HC_U32_MASK.snapshot()
}

/// All currently-disabled skill ids. Delegates to the framework
/// Tracker's canonical `disabled_skills` set.
/// Stays here because it exposes Grounded 2's catalog state to this mod's debug endpoint.
pub(crate) fn disabled_skills_snapshot() -> Vec<&'static str> {
    crate::rpg::tracker::TRACKER.disabled_skills().snapshot()
}

// Per-(offset) vanilla `UHealthComponent` u32 mask values.
pub(crate) static VANILLA_HC_U32_MASK: ueforge::rpg::VanillaCache<usize, u32> =
    ueforge::rpg::VanillaCache::new();

/// True when the skill should currently apply at its stored level.
/// Reads through the framework Tracker's internal toggle set.
/// no separate game-side mirror.
/// Stays here because it adapts Grounded 2's skill identifiers to its UI callbacks.
pub fn is_skill_enabled(skill_id: &str) -> bool {
    !crate::rpg::tracker::TRACKER
        .disabled_skills()
        .is_disabled(skill_id)
}

/// Set the enabled flag for `skill_id`. Returns the new state.
/// Also re-applies the skill so the change is immediate.
/// Stays here because Grounded 2's UI decides when a catalog skill is toggled;
/// Modforge owns the toggle state and effect dispatch.
pub fn set_skill_enabled(skill_id: &'static str, enabled: bool) -> bool {
    let now_disabled = crate::rpg::tracker::TRACKER
        .disabled_skills()
        .set(skill_id, !enabled);
    crate::rpg::tracker::TRACKER.reapply_one(skill_id);
    !now_disabled
}

// Per-offset vanilla baselines for movement / global-data /
// max-health Effects. Keyed by offset; populated lazily on first
// apply.
pub(crate) static MOVEMENT_VANILLA: ueforge::rpg::VanillaCache<usize, f32> =
    ueforge::rpg::VanillaCache::new();
pub(crate) static GLOBAL_DATA_VANILLA: ueforge::rpg::VanillaCache<usize, f32> =
    ueforge::rpg::VanillaCache::new();
pub(crate) static MAX_HEALTH_VANILLA: ueforge::rpg::VanillaCache<usize, f32> =
    ueforge::rpg::VanillaCache::new();

/// Read and store the untouched hunger/thirst drain rates. Run
/// at init before any patching.
/// Stays here because the component class and field offsets are Grounded 2 facts;
/// Ueforge owns object discovery and field reads.
pub fn capture_vanilla() {
    if VANILLA_HUNGER.get().is_some() && VANILLA_THIRST.get().is_some() {
        return;
    }
    let Some(rt) = ue::try_runtime() else { return };
    let Some(survival_class) = ue::find_class_fast("SurvivalComponent") else {
        return;
    };
    let view = unsafe { GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return;
    }
    for obj in view.iter() {
        if !obj.is_a(survival_class) || !obj.is_default_object() {
            continue;
        }
        let h = read_f32(obj, survival::HUNGER_ADJUSTMENT_OFFSET);
        let t = read_f32(obj, survival::THIRST_ADJUSTMENT_OFFSET);
        if h.is_finite() && h != 0.0 {
            let _ = VANILLA_HUNGER.set(h);
        }
        if t.is_finite() && t != 0.0 {
            let _ = VANILLA_THIRST.set(t);
        }
        ueforge::log!(
            "rpg/apply: captured vanilla hunger={:.4}, thirst={:.4}",
            h,
            t
        );
        return;
    }
}

// ---------------------------------------------------------------
// Class refs used by the apply path. PlayerRef wraps the
// (base_class, BP_filter) pair canonically.
// ---------------------------------------------------------------

pub(crate) static PLAYER: ue::PlayerRef =
    ue::PlayerRef::new("SurvivalCharacter", Some("BP_SurvivalPlayerCharacter"));
pub(crate) static CLASS_SURVIVAL_COMPONENT: ue::ClassRef = ue::ClassRef::new("SurvivalComponent");
pub(crate) static CLASS_SURVIVAL_MODE_MANAGER_COMPONENT: ue::ClassRef =
    ue::ClassRef::new("SurvivalModeManagerComponent");
pub(crate) static CLASS_SURVIVAL_GAME_MODE_SETTINGS: ue::ClassRef =
    ue::ClassRef::new("SurvivalGameModeSettings");

pub(crate) static CLASS_GLOBAL_COMBAT_DATA: ue::ClassRef = ue::ClassRef::new("GlobalCombatData");

/// Runs an operation against every live Grounded 2 player character.
/// Stays here because it supplies this game's player-class filter to Ueforge's reusable walker.
pub(crate) fn apply_to_live_player_characters(f: impl FnMut(&UObject)) -> usize {
    PLAYER.for_each_live(f)
}
