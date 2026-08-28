//! Combat-XP levelling: skill catalog + tracker + ops + slot
//! poller (docs/plan.md section 5; shape mirrors
//! wwm-mod/src/skills.rs).
//!
//! Skills anchor on live-proven fields (tests/
//! research_levelling.rs, 2026-08-08):
//! - vitality: PlayerHealth.MaxHealth, a STATIC property.
//! - regeneration: PlayerHealth.HealthRecoveryPerMinute, static.
//! - heavy_hands: PunchController.Min/MaxPunchDamage, instance
//!   properties on the local player's controller.
//!
//! Save-slot identity: LoadManager (singleton) exposes
//! IsGameLoaded + LoadedGameFolderPath; the slot key is the
//! save folder's trailing segment (e.g. "SaveGame_2").
//!
//! IL2CPP rule: every game-touching call runs on the main
//! thread. Effects apply from the HTTP thread (skill ops) and
//! the poller thread, so each effect body and the slot resolver
//! hop through MAIN_QUEUE.

use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use modforge::rpg::EffectDef;
use modforge::rpg::poller::{PollerHandle, SlotPoller};
use modforge::rpg::vanilla::VanillaCache;
use modforge::rpg::xp::Curve;

use unityforge::main_thread_queue::MAIN_QUEUE;
use unityforge::mono::MonoType;
use unityforge::rpg::{
    SkillDef, SkillRegistry, Tracker, UnityGuardedMainThreadEffect,
    UnityInstancePropMultiplyEffect, UnityStaticPropAdditiveEffect,
};

// ---- Effects --------------------------------------------------------

/// Crash bisection switch. Default ON now that the catalog
/// carries only the proven-safe instance-property effect; the
/// effects_enable op can still disarm for future bisections.
static EFFECTS_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
const EFFECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Reports whether Schedule 1's researched skill writes are armed.
/// Stays here because this switch isolates crashes in this game's effects; Modforge owns effects and Unityforge owns Unity writes.
fn effects_enabled() -> bool {
    EFFECTS_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

#[allow(dead_code)]
static VITALITY_VANILLA: VanillaCache<&'static str, f32> = VanillaCache::new();
#[allow(dead_code)]
static VITALITY_INNER: UnityStaticPropAdditiveEffect = UnityStaticPropAdditiveEffect::new(
    "Il2CppScheduleOne.PlayerScripts.Health.PlayerHealth",
    "MaxHealth",
    400.0,
    "max health",
    &VITALITY_VANILLA,
);
#[allow(dead_code)]
static VITALITY_EFFECT: UnityGuardedMainThreadEffect<UnityStaticPropAdditiveEffect> =
    UnityGuardedMainThreadEffect::new("vitality", EFFECT_TIMEOUT, effects_enabled, &VITALITY_INNER);

#[allow(dead_code)]
static REGENERATION_VANILLA: VanillaCache<&'static str, f32> = VanillaCache::new();
#[allow(dead_code)]
static REGENERATION_INNER: UnityStaticPropAdditiveEffect = UnityStaticPropAdditiveEffect::new(
    "Il2CppScheduleOne.PlayerScripts.Health.PlayerHealth",
    "HealthRecoveryPerMinute",
    100.0,
    "health per minute",
    &REGENERATION_VANILLA,
);
#[allow(dead_code)]
static REGENERATION_EFFECT: UnityGuardedMainThreadEffect<UnityStaticPropAdditiveEffect> =
    UnityGuardedMainThreadEffect::new(
        "regeneration",
        EFFECT_TIMEOUT,
        effects_enabled,
        &REGENERATION_INNER,
    );

static HEAVY_HANDS_VANILLA: VanillaCache<&'static str, f32> = VanillaCache::new();
static HEAVY_HANDS_INNER: UnityInstancePropMultiplyEffect = UnityInstancePropMultiplyEffect::new(
    "Il2CppScheduleOne.Combat.PunchController",
    &["MinPunchDamage", "MaxPunchDamage"],
    4.0,
    "punch damage",
    &HEAVY_HANDS_VANILLA,
);
static HEAVY_HANDS_EFFECT: UnityGuardedMainThreadEffect<UnityInstancePropMultiplyEffect> =
    UnityGuardedMainThreadEffect::new(
        "heavy_hands",
        EFFECT_TIMEOUT,
        effects_enabled,
        &HEAVY_HANDS_INNER,
    );

// ---- Catalog --------------------------------------------------------

// vitality + regeneration are ON ICE (2026-08-08): their
// PlayerHealth statics are field-backed and BOTH write paths
// (generated setter AND direct il2cpp_field_static_set_value)
// crash or corrupt the 0.4.6f12 game; fallout of the patched
// interop generator skipping metadata init on scan failure.
// They return when the generator fix lands (docs/todo.md).
// Instance-property writes are proven safe (probe 2026-08-08:
// MinPunchDamage 20 -> 12 -> 20 round trip, game healthy).
pub static CATALOG: SkillRegistry = SkillRegistry::new(&[SkillDef {
    id: "heavy_hands",
    display_name: "Heavy Hands",
    max_level: 100,
    effect: EffectDef::new("InstancePropMultiply", &HEAVY_HANDS_EFFECT),
    trigger: &modforge::rpg::ON_SLOT_CHANGE,
}]);

// ---- Tracker --------------------------------------------------------

// Endless-feel per the operator ("i hate level caps"): gentle
// exponent, 1024 (the framework's hard limit) is unreachable in
// practice at 25 XP per kill.
pub static TRACKER: Tracker = Tracker::new(&CATALOG, Curve::new(50.0, 1.3, 1024), "schedule1-mod");

/// Spend earned points automatically, each on the currently
/// lowest-level skill (operator's choice: zero friction while
/// farming; the phone app replaces this later).
/// Stays here because automatic spending is Schedule 1 progression policy; Modforge owns the tracker and skill registry.
pub fn auto_spend(points: u32) {
    TRACKER.spend_lowest_skill_points(points);
}

// ---- Slot poller ----------------------------------------------------

static POLLER: OnceLock<PollerHandle> = OnceLock::new();

/// True once the loaded game has settled (both crashes on
/// 2026-08-08 landed in the just-loaded window, so nothing
/// game-touching runs until the load is 10s old).
pub static SETTLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The loaded save's folder name ("SaveGame_2") from the
/// LoadManager singleton, main-thread queued; None at the menu
/// and during the first 10s after a load.
/// Stays here because the class, fields, folder identity, and settling delay are Schedule 1 facts; Unityforge owns managed access.
fn resolve_slot() -> Option<String> {
    let slot = MAIN_QUEUE
        .run("slot_resolve", Duration::from_secs(2), || {
            let ty = MonoType::find("Il2CppScheduleOne.Persistence.LoadManager")?;
            let obj = ty.singleton_instance()?;
            if obj.read_field("IsGameLoaded").ok()?.as_bool() != Some(true) {
                return None;
            }
            if obj
                .read_field("TimeSinceGameLoaded")
                .ok()?
                .as_f64()
                .is_none_or(|t| t < 10.0)
            {
                return None;
            }
            let path = obj.read_field("LoadedGameFolderPath").ok()?;
            let path = path.as_str()?;
            let slot = path
                .trim_end_matches(['/', '\\'])
                .rsplit(['/', '\\'])
                .next()?
                .to_string();
            if slot.is_empty() { None } else { Some(slot) }
        })
        .ok()
        .flatten();
    SETTLED.store(slot.is_some(), std::sync::atomic::Ordering::Relaxed);
    slot
}

// ---- Install --------------------------------------------------------

/// Seeds Schedule 1's proven stat baselines and starts its persistent skill tracker.
/// Stays here because the values, save resolver, and selected catalog are game-specific; the frameworks own tracking and polling.
pub fn install() {
    // Seed the proven vanilla values (live-read 2026-08-08 on
    // 0.4.6f12) so a hot reload never recaptures already-boosted
    // values as baselines (the poisoning seen same day). A game
    // update that changes these must re-verify via
    // tests/levelling.rs instance_prop_probe.
    HEAVY_HANDS_VANILLA.set_if_unset("MinPunchDamage", 20.0);
    HEAVY_HANDS_VANILLA.set_if_unset("MaxPunchDamage", 35.0);

    unityforge::rpg::ops::register(&TRACKER);
    register_ops();
    let handle = SlotPoller::spawn(
        Duration::from_secs(2),
        resolve_slot,
        |slot| TRACKER.activate_slot(slot),
        || TRACKER.deactivate_slot(),
    );
    let _ = POLLER.set(handle);
}

/// Exposes Schedule 1's crash-bisection switch through the shared control plane.
/// Stays here because the operation controls this mod's local safety guard; Modforge owns operation registration.
fn register_ops() {
    OP_REGISTRY.register_many([OpDef::new(
        "effects_enable",
        "Crash bisection: arm or disarm the skill effect bodies (inert by default)",
        "{on: bool}",
        |args| {
            let on = args.get("on").and_then(Json::as_bool).unwrap_or(false);
            EFFECTS_ENABLED.store(on, std::sync::atomic::Ordering::Relaxed);
            Ok(json!({"effects_enabled": on}))
        },
    )]);
}
