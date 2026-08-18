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

use modforge::args::arg_str;
use modforge::ops::{OP_REGISTRY, OpDef};
use modforge::rpg::poller::{PollerHandle, SlotPoller};
use modforge::rpg::progress::sqrt_progress;
use modforge::rpg::vanilla::VanillaCache;
use modforge::rpg::xp::Curve;
use modforge::rpg::{Effect, EffectDef, TriggerCtx, format};

use unityforge::main_thread_queue::MAIN_QUEUE;
use unityforge::mono::{self, MonoType};
use unityforge::rpg::engine::UnityEngine;
use unityforge::rpg::{SkillDef, SkillRegistry, Tracker};

// ---- Effects --------------------------------------------------------

/// Crash bisection switch. Default ON now that the catalog
/// carries only the proven-safe instance-property effect; the
/// effects_enable op can still disarm for future bisections.
static EFFECTS_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

fn effects_enabled() -> bool {
    EFFECTS_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// `vanilla + max_bonus * progress` on a STATIC property
/// (get_X/set_X static pair), main-thread queued. On ice with
/// vitality/regeneration until the generator fix (see the
/// CATALOG note); kept so they slot straight back in.
#[allow(dead_code)]
struct StaticPropAdditiveEffect {
    class_name: &'static str,
    prop_name: &'static str,
    max_bonus: f32,
    format_word: &'static str,
    vanilla: &'static VanillaCache<&'static str, f32>,
}

impl Effect<UnityEngine> for StaticPropAdditiveEffect {
    fn apply(&self, level: u32, max_level: u32, _ctx: &TriggerCtx<'_, UnityEngine>) {
        if !effects_enabled() {
            return;
        }
        let progress = sqrt_progress(level, max_level);
        let (class, prop, bonus) = (self.class_name, self.prop_name, self.max_bonus);
        let vanilla = self.vanilla;
        let _ = MAIN_QUEUE.run(prop, Duration::from_secs(2), move || {
            let cur = mono::invoke_static(class, &format!("get_{prop}"), &json!([]))
                .ok()
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let Some(cur) = cur else { return };
            let baseline = if cur.is_finite() && cur != 0.0 {
                vanilla.get_or_init(prop, cur)
            } else {
                vanilla.get(prop).unwrap_or(cur)
            };
            let target = baseline + bonus * progress;
            let _ = mono::invoke_static(class, &format!("set_{prop}"), &json!([target]));
        });
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        format::format_additive_f32_as_int(self.max_bonus, level, max_level, self.format_word)
    }
}

/// `vanilla * (1 + max_bonus * progress)` on instance
/// properties of the (single) live instance of a class,
/// main-thread queued. Used for the local player's
/// PunchController damage pair.
struct InstancePropMultiplyEffect {
    class_name: &'static str,
    prop_names: &'static [&'static str],
    max_bonus: f32,
    format_word: &'static str,
    vanilla: &'static VanillaCache<&'static str, f32>,
}

impl Effect<UnityEngine> for InstancePropMultiplyEffect {
    fn apply(&self, level: u32, max_level: u32, _ctx: &TriggerCtx<'_, UnityEngine>) {
        if !effects_enabled() {
            return;
        }
        let mult = 1.0 + self.max_bonus * sqrt_progress(level, max_level);
        let (class, props) = (self.class_name, self.prop_names);
        let vanilla = self.vanilla;
        let _ = MAIN_QUEUE.run(class, Duration::from_secs(2), move || {
            let Some(ty) = MonoType::find(class) else { return };
            let Ok(walked) = ty.walk(false) else { return };
            let Some(h) = walked
                .as_array()
                .and_then(|a| a.first())
                .and_then(|i| i["handle"].as_i64())
            else {
                return;
            };
            // SAFETY: handle just acquired by the walk; Drop
            // releases it when this scope ends.
            let obj = unsafe {
                unityforge::mono::MonoObject::from_handle(unityforge::bridge::MonoHandle(h as i32))
            };
            for prop in props {
                let cur = obj
                    .invoke(&format!("get_{prop}"), &json!([]))
                    .ok()
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32);
                let Some(cur) = cur else { continue };
                let baseline = if cur.is_finite() && cur != 0.0 {
                    vanilla.get_or_init(prop, cur)
                } else {
                    vanilla.get(prop).unwrap_or(cur)
                };
                let _ = obj.invoke(&format!("set_{prop}"), &json!([baseline * mult]));
            }
        });
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        format::format_multiplier(self.max_bonus, level, max_level, self.format_word)
    }
}

#[allow(dead_code)]
static VITALITY_VANILLA: VanillaCache<&'static str, f32> = VanillaCache::new();
#[allow(dead_code)]
static VITALITY_EFFECT: StaticPropAdditiveEffect = StaticPropAdditiveEffect {
    class_name: "Il2CppScheduleOne.PlayerScripts.Health.PlayerHealth",
    prop_name: "MaxHealth",
    max_bonus: 400.0,
    format_word: "max health",
    vanilla: &VITALITY_VANILLA,
};

#[allow(dead_code)]
static REGENERATION_VANILLA: VanillaCache<&'static str, f32> = VanillaCache::new();
#[allow(dead_code)]
static REGENERATION_EFFECT: StaticPropAdditiveEffect = StaticPropAdditiveEffect {
    class_name: "Il2CppScheduleOne.PlayerScripts.Health.PlayerHealth",
    prop_name: "HealthRecoveryPerMinute",
    max_bonus: 100.0,
    format_word: "health per minute",
    vanilla: &REGENERATION_VANILLA,
};

static HEAVY_HANDS_VANILLA: VanillaCache<&'static str, f32> = VanillaCache::new();
static HEAVY_HANDS_EFFECT: InstancePropMultiplyEffect = InstancePropMultiplyEffect {
    class_name: "Il2CppScheduleOne.Combat.PunchController",
    prop_names: &["MinPunchDamage", "MaxPunchDamage"],
    max_bonus: 4.0,
    format_word: "punch damage",
    vanilla: &HEAVY_HANDS_VANILLA,
};

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
pub fn auto_spend(points: u32) {
    for _ in 0..points {
        let lowest = TRACKER.with_state(|s| {
            CATALOG
                .iter()
                .filter(|sk| s.level_of(sk.id) < sk.max_level)
                .min_by_key(|sk| s.level_of(sk.id))
                .map(|sk| sk.id)
        });
        let Some(Some(id)) = lowest else { return };
        if TRACKER.spend_skill_points(id, 1) == 0 {
            return;
        }
    }
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

fn register_ops() {
    OP_REGISTRY.register_many([
        OpDef::new(
            "skill_state",
            "Snapshot of the schedule1-mod skill state (xp, level, points, per-skill levels)",
            "{}",
            |_args| {
                let snapshot = TRACKER.with_state(|s| {
                    let mut skills = serde_json::Map::new();
                    for skill in CATALOG.iter() {
                        skills.insert(
                            skill.id.to_string(),
                            json!({
                                "level": s.level_of(skill.id),
                                "max_level": skill.max_level,
                                "effect": TRACKER.format_effect(skill, s.level_of(skill.id)),
                            }),
                        );
                    }
                    json!({
                        "xp": s.xp,
                        "level": s.level,
                        "skill_points": s.skill_points,
                        "skills": Json::Object(skills),
                    })
                });
                Ok(snapshot.unwrap_or_else(|| json!({"active": false, "msg": "no slot active"})))
            },
        ),
        OpDef::new(
            "skill_add_xp",
            "DEBUG: manually award XP",
            "{amount: u64}",
            |args| {
                let amount = args.get("amount").and_then(Json::as_u64).unwrap_or(0);
                let Some(result) = TRACKER.record_xp(amount) else {
                    return Err("no slot active or save failed".into());
                };
                Ok(json!({
                    "awarded": result.awarded,
                    "total_xp": result.total_xp,
                    "old_level": result.old_level,
                    "new_level": result.new_level,
                    "points_gained": result.points_gained,
                }))
            },
        ),
        OpDef::new(
            "skill_levelup",
            "Spend points on a skill",
            "{id: str, count?: u32}",
            |args| {
                let id = arg_str(args, "id")?;
                let count = args.get("count").and_then(Json::as_u64).unwrap_or(1) as u32;
                let spent = TRACKER.spend_skill_points(id, count);
                Ok(json!({
                    "id": id,
                    "spent": spent,
                    "level": TRACKER.with_state(|s| s.level_of(id)).unwrap_or(0),
                }))
            },
        ),
        OpDef::new(
            "effects_enable",
            "Crash bisection: arm or disarm the skill effect bodies (inert by default)",
            "{on: bool}",
            |args| {
                let on = args.get("on").and_then(Json::as_bool).unwrap_or(false);
                EFFECTS_ENABLED.store(on, std::sync::atomic::Ordering::Relaxed);
                Ok(json!({"effects_enabled": on}))
            },
        ),
        OpDef::new(
            "skill_grant_points",
            "DEBUG: grant skill points without earning them",
            "{n: u32}",
            |args| {
                let n = args.get("n").and_then(Json::as_u64).unwrap_or(1) as u32;
                let ok = TRACKER.debug_grant_skill_points(n);
                Ok(json!({"granted": ok, "n": n}))
            },
        ),
    ]);
}
