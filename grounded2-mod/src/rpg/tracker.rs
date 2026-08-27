// Bbp tracker shim. Owns the static `ueforge::rpg::Tracker` and
// exposes the g2rpg-side surface that the rest of the mod calls
// (kill_hook, world_loader, debug, tab). Every skill-state
// operation routes through ueforge::rpg::Tracker, which owns
// state, persistence, and the apply dispatch (via the EffectDef
// on each catalog row. No game-side applier needed any more).

use ueforge::rpg::{SkillDef, SkillsState, Tracker};

use crate::rpg::skills::CATALOG;
use crate::rpg::xp::CURVE;
use crate::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillSource {
    Player,
    Buggy,
}

pub static TRACKER: Tracker = Tracker::new(&CATALOG, CURVE, "saves");

/// Loads the selected playthrough's RPG state and applies its Grounded 2 settings.
/// Stays here because it combines this game's Buggy XP setting with Modforge's reusable tracker.
pub fn activate_slot(slot: String, settings: Settings) {
    cache_buggy_kill_multiplier(settings.rpg.buggy_kill_xp_multiplier.max(0.0));
    TRACKER.activate_slot(slot);
}

/// Releases the active Grounded 2 playthrough state when the player leaves the world.
/// Stays here because it is this mod's world-lifecycle adapter to Modforge's tracker.
pub fn deactivate_slot() {
    TRACKER.deactivate_slot();
}

/// Reports which Grounded 2 playthrough currently owns the RPG state.
/// Stays here because it exposes this mod's tracker to its debug and world-loading code.
pub fn current_slot() -> Option<String> {
    TRACKER.current_slot()
}

/// Lets Grounded 2 features inspect the active character progression without copying it.
/// Stays here because it is this mod's local access point to Modforge's authoritative state.
pub fn with_state<R>(f: impl FnOnce(&SkillsState) -> R) -> Option<R> {
    TRACKER.with_state(f)
}

/// Buys one level of a Grounded 2 skill when a point is available.
/// Stays here because it accepts this mod's catalog entry; Modforge owns spending and persistence.
pub fn spend_skill_point(skill: &SkillDef) -> bool {
    TRACKER.spend_skill_points(skill.id, 1) > 0
}

/// Buys up to the requested number of levels for a Grounded 2 skill.
/// Stays here because it accepts this mod's catalog entry; Modforge owns spending and persistence.
pub fn spend_skill_points(skill: &SkillDef, count: u32) -> u32 {
    TRACKER.spend_skill_points(skill.id, count)
}

/// Refunds one level of a Grounded 2 skill.
/// Stays here because it accepts this mod's catalog entry; Modforge owns refunds and persistence.
pub fn refund_skill_point(skill: &SkillDef) -> bool {
    TRACKER.refund_skill_points(skill.id, 1) > 0
}

/// Refunds up to the requested number of levels for a Grounded 2 skill.
/// Stays here because it accepts this mod's catalog entry; Modforge owns refunds and persistence.
pub fn refund_skill_points(skill: &SkillDef, count: u32) -> u32 {
    TRACKER.refund_skill_points(skill.id, count)
}

/// Reapplies one Grounded 2 skill after its enabled state changes.
/// Stays here because it exposes this mod's catalog through the Modforge tracker.
pub fn reapply_one(skill_id: &str) {
    TRACKER.reapply_one(skill_id);
}

/// Reapplies every purchased Grounded 2 skill to the live player and class defaults.
/// Stays here because it exposes this mod's catalog through the Modforge tracker.
pub fn reapply_all() -> bool {
    TRACKER.reapply_all()
}

/// Grants skill points through Grounded 2's debug controls.
/// Stays here because it is this mod's debug surface; Modforge owns the state mutation.
pub fn debug_grant_skill_points(count: u32) -> bool {
    TRACKER.debug_grant_skill_points(count)
}

/// Called from the kill hook on every confirmed creature kill.
/// Awards XP via the per-creature lookup, scaled by `source`.
/// Stays here because Grounded 2 defines creature rewards and partial credit for Buggy kills;
/// Modforge owns XP accounting, leveling, and persistence.
pub fn record_kill(creature_class_name: &str, source: KillSource) {
    let base = crate::rpg::xp::xp_for_creature(creature_class_name);
    let scaled = match source {
        KillSource::Player => base,
        KillSource::Buggy => {
            let mult = TRACKER
                .with_state(|_| {
                    // Settings live inside GameApplier; expose via a
                    // dedicated accessor if the multiplier ever
                    // varies per-kill. For now the multiplier is
                    // captured at slot activate and survives until
                    // deactivate.
                })
                .map(|_| 1.0_f32)
                .unwrap_or(1.0);
            // We don't currently expose Settings through Tracker;
            // the multiplier is read from a process-global cache.
            (base as f32 * mult * settings_buggy_kill_xp_multiplier()) as u32
        }
    };
    let xp = TRACKER.record_xp(scaled as u64);
    ueforge::log!(
        "rpg/kill: ({creature_class_name}) source={source:?} +{scaled} XP -> total {} (level {})",
        xp.map(|r| r.total_xp).unwrap_or(0),
        xp.map(|r| r.new_level).unwrap_or(0),
    );
}

// ---------------------------------------------------------------
// Settings escape hatch for record_kill's buggy-kill multiplier.
// Tracker doesn't expose Settings; we cache the per-slot
// multiplier in a static at activate_slot. Game-specific.
// ---------------------------------------------------------------

use std::sync::atomic::{AtomicU32, Ordering};

static BUGGY_MULT_BITS: AtomicU32 = AtomicU32::new(0x3F800000); // 1.0_f32

/// Caches the Grounded 2 setting used to scale XP from Buggy kills.
/// Stays here because tame Buggy kill credit is specific to this game's combat rules.
pub fn cache_buggy_kill_multiplier(v: f32) {
    BUGGY_MULT_BITS.store(v.to_bits(), Ordering::Relaxed);
}

/// Reads the active Grounded 2 multiplier for Buggy kill credit.
/// Stays here because tame Buggy kill credit is specific to this game's combat rules.
fn settings_buggy_kill_xp_multiplier() -> f32 {
    f32::from_bits(BUGGY_MULT_BITS.load(Ordering::Relaxed))
}
