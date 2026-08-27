//! Scaling NPC spawner (research.md 25, 26.3).
//!
//! The placed NPCs in each streamed map square are anchors.
//! When a square streams in, roll a threat budget that grows
//! with the save's shining count (EmissionsPast on the world
//! generators) and spend it on extra NPCs spawned near the
//! anchors via SpawnAIFromClass on the game thread.
//!
//! Randomness is the point: a square can roll nothing (the
//! quiet chance), a few copies of its own NPCs, cross-biome
//! escalations from the live class pool, or rarely a pack.
//! Average extras reach the square's own vanilla count (a
//! doubling) at DOUBLE_AT_EMISSIONS.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use modforge::storyteller::{
    EncounterAnchor, EncounterChance, EncounterConfig, EncounterPlan, EncounterPlanner,
};
use ueforge::ue::streaming::NewLevels;
use ueforge::ue::{self, UObject, read_at};

/// One knob for overall intensity; multiplies the budget curve.
const INTENSITY: f64 = 1.0;
/// Average extras equal the square's vanilla count here.
const DOUBLE_AT_EMISSIONS: f64 = 30.0;
/// Chance a square rolls nothing at all, regardless of level.
const QUIET_CHANCE: f64 = 0.20;
/// Per-square cap on extras (packs count toward it).
const PER_SQUARE_CAP: usize = 8;
/// Session cap on everything we spawn.
const SESSION_CAP: u64 = 60;
/// Escalation chance per extra: base + per-emission, capped.
const ESCALATE_BASE: f64 = 0.10;
const ESCALATE_PER_EMISSION: f64 = 0.01;
const ESCALATE_CAP: f64 = 0.50;
/// Pack chance per square: base + per-emission, capped.
const PACK_BASE: f64 = 0.05;
const PACK_PER_EMISSION: f64 = 0.002;
const PACK_CAP: f64 = 0.15;
/// Spawn scatter around the anchor, in engine units.
const JITTER_MIN: f64 = 300.0;
const JITTER_MAX: f64 = 800.0;
/// How often the watcher looks for newly streamed squares.
const POLL: Duration = Duration::from_secs(5);

/// EmissionsPast on BP_WorldGeneration_Base_C (research.md 19.2).
const EMISSIONS_PAST_OFFSET: usize = 0x2F8;

/// Where MISERY keeps its loaded squares, all measured live
/// (worldgen.md 10). The generator holds an array of streaming
/// levels; each of those points at the `ULevel` it has loaded,
/// whose name IS the square.
///
/// Four generators exist, one per area, and only the active one
/// has a non-empty array, which is how the shared code picks it.
pub const STREAMER: ueforge::ue::streaming::LevelStreamer =
    ueforge::ue::streaming::LevelStreamer {
        class: "BP_WorldGeneration_Base_C",
        levels: 0x2E8,
        loaded_level: 0x158,
    };

/// Which squares have appeared since the last check. The shared
/// watcher does the remembering.
static WATCH: std::sync::Mutex<Option<NewLevels>> = std::sync::Mutex::new(None);

/// Classes never used as escalations (harmless or friendly).
const NO_ESCALATE: &[&str] = &["Tamed", "DeerNeutral", "Boar"];

static LAST_EMISSIONS: AtomicI32 = AtomicI32::new(-1);
/// Test override for the emission level; negative = use live.
static EMISSIONS_OVERRIDE: AtomicI32 = AtomicI32::new(-1);

static PLANNER: std::sync::Mutex<Option<EncounterPlanner<String>>> = std::sync::Mutex::new(None);

/// The save's shining count: max EmissionsPast across the four
/// world generators (only the active one accumulates).
/// Reads how many Shinings the current world has survived to scale extra enemies.
/// Stays here because the progression source is a MISERY world-generation Blueprint field.
fn emission_level() -> i32 {
    let ov = EMISSIONS_OVERRIDE.load(Ordering::Relaxed);
    if ov >= 0 {
        return ov;
    }
    // Off the cached streamer pointer. This used to SEARCH the
    // whole object list, 100 ms, to read one i32 off an object
    // we already had (docs/performance.md).
    STREAMER.field::<i32>(EMISSIONS_PAST_OFFSET).unwrap_or(0)
}

/// The hostile NPCs each loaded square already has.
///
/// `WorldPresets` is MISERY's package for its map squares, which
/// excludes the hub and anything the mod spawned itself. The
/// counting is `ue::actor::count_by_level`.
fn census() -> HashMap<String, usize> {
    ueforge::ue::actor::count_by_level("BP_MasterAICharacter_C", Some("WorldPresets"))
}

/// Extras per square, as a share of what the square already has:
/// the mean reaches the square's own vanilla count (a doubling)
/// at DOUBLE_AT_EMISSIONS. The curve, the quiet chance and the
/// cap are `modforge::roll`, which is unit-tested; what stays
/// here is only what MISERY spends the budget on.
const ENCOUNTER_CONFIG: EncounterConfig = EncounterConfig {
    budget: modforge::roll::Budget {
        quiet_chance: QUIET_CHANCE,
        at_zero: 0.0,
        per_level: 1.0 / DOUBLE_AT_EMISSIONS,
        intensity: INTENSITY,
        max: PER_SQUARE_CAP,
    },
    escalation: EncounterChance {
        base: ESCALATE_BASE,
        per_level: ESCALATE_PER_EMISSION,
        max: ESCALATE_CAP,
    },
    pack: EncounterChance {
        base: PACK_BASE,
        per_level: PACK_PER_EMISSION,
        max: PACK_CAP,
    },
    pack_min: 3,
    pack_max: 5,
    session_cap: SESSION_CAP as usize,
    scatter_min: JITTER_MIN,
    scatter_max: JITTER_MAX,
    height_offset: 100.0,
};

/// Background watcher. Rolls a plan for each newly streamed
/// square and executes it on the game thread.
/// Starts adaptive enemy spawning and exposes its player-facing controls.
/// Stays here because it composes shared polling and rolling around MISERY enemies and map squares.
pub fn install() {
    register_ops();
    // Stoppable so the DLL can unload; a raw thread would keep
    // running in freed code and crash a hot reload.
    std::mem::forget(modforge::rpg::poller::spawn_interval(
        "misery-spawning",
        POLL,
        // Reads live actors, so it runs on the game thread.
        ueforge::game_thread::each_tick(watcher),
    ));
}

/// Detects newly loaded MISERY squares and schedules their extra enemy encounters.
/// Stays here because polling the live Unreal world and dispatching game-thread work are MISERY integration.
fn watcher() {
    if ue::try_runtime().is_none() {
        return;
    }

    // Which squares appeared since last time. Asked of the
    // generator, not worked out by reading every object in the
    // game: a cached pointer and two array reads instead of
    // 174,000 objects and 132 ms (worldgen.md 10).
    //
    // A tick with nothing new stops here, and that is the whole
    // point.
    let (live, fresh) = {
        let mut guard = WATCH.lock().unwrap_or_else(|e| e.into_inner());
        let watch = guard.get_or_insert_with(|| NewLevels::new(STREAMER));
        let fresh = watch.since_last();
        (watch.all(), fresh)
    };
    if fresh.is_empty() {
        return;
    }
    {
        let mut guard = PLANNER.lock().unwrap_or_else(|e| e.into_inner());
        let planner = guard.get_or_insert_with(|| EncounterPlanner::new(ENCOUNTER_CONFIG));
        planner.retain_places(|square| live.iter().any(|l| l == square));
        if planner.is_full() {
            return;
        }
    }

    // Only now is a search worth paying for, and only for the
    // squares that actually appeared.
    let squares = census();
    let emissions = emission_level();
    LAST_EMISSIONS.store(emissions, Ordering::Relaxed);

    for (square, vanilla) in squares.iter().filter(|(s, _)| fresh.contains(s)) {
        let plan = {
            let mut guard = PLANNER.lock().unwrap_or_else(|e| e.into_inner());
            guard
                .get_or_insert_with(|| EncounterPlanner::new(ENCOUNTER_CONFIG))
                .plan_place(square.clone(), *vanilla, emissions as f64)
        };
        let Some(plan) = plan else {
            continue;
        };
        let total = plan.copies + plan.escalations + plan.pack;
        ueforge::log::log(format_args!(
            "spawning: {} vanilla={vanilla} emissions={emissions} copies={} escalations={} pack={}",
            ueforge::ue::actor::short_name(square),
            plan.copies,
            plan.escalations,
            plan.pack,
        ));
        if total == 0 {
            continue;
        }
        // Not `enqueue`: this watcher already runs ON the
        // game thread, so queueing here would wait for a
        // drain that cannot start until we return. `run`
        // executes in place when we are already there.
        let r = ueforge::game_thread::run(move || execute_plan(&plan), Duration::from_secs(10));
        if let Err(e) = r {
            ueforge::log::log(format_args!("spawning: plan failed: {e}"));
        }
    }
}

/// Runs on the game thread. Re-censuses the square (pointers
/// from the watcher could be stale), then spawns.
/// Places a planned group of extra enemies around the chosen loaded square.
/// Stays here because it reads MISERY's live enemies and executes Modforge requests through Ueforge.
fn execute_plan(plan: &EncounterPlan<String>) -> Result<serde_json::Value, String> {
    #[derive(Clone)]
    struct EnemyClass {
        ptr: u64,
        name: String,
    }

    let mut anchors = Vec::new();
    let mut pool = Vec::new();
    for p in ueforge::ue::actor::find_actors_by_chain("BP_MasterAICharacter_C") {
        // SAFETY: p is live; we are on the game thread, nothing
        // streams out mid-frame.
        let obj = unsafe { &*(p as *const UObject) };
        let class_name = obj
            .class()
            .map(|c| c.as_object().name())
            .unwrap_or_default();
        let class_ptr = unsafe { read_at::<u64>(p, 0x10) };
        let class = (class_ptr != 0).then(|| EnemyClass {
            ptr: class_ptr,
            name: class_name.clone(),
        });
        if class_ptr != 0 && !NO_ESCALATE.iter().any(|n| class_name.contains(n)) {
            pool.push(class.clone().expect("non-null class was captured"));
        }
        let full = obj.full_name();
        let in_this_square = ueforge::ue::actor::level_of(&full) == Some(plan.place.as_str());
        if in_this_square {
            anchors.push(EncounterAnchor {
                value: p,
                class,
                // SAFETY: p is a live actor on the game thread.
                position: unsafe { ueforge::ue::transform::world_location(p) },
            });
        }
    }
    if anchors.is_empty() {
        return Err("square has no live anchors any more".into());
    }
    if pool.is_empty() {
        return Err("no escalation pool".into());
    }
    let world_ctx = ueforge::ue::actor::find_actors_by_chain("BP_SGKMasterCharacter_C")
        .into_iter()
        .next()
        .ok_or("no player for world context")?;

    let mut guard = PLANNER.lock().unwrap_or_else(|e| e.into_inner());
    let planner = guard.get_or_insert_with(|| EncounterPlanner::new(ENCOUNTER_CONFIG));
    let spawned = planner.execute(
        plan,
        &anchors,
        &pool,
        |class, count| {
            ueforge::log::log(format_args!(
                "spawning: PACK of {count} x {} at {}",
                class.name,
                ueforge::ue::actor::short_name(&plan.place)
            ));
        },
        |request| {
            let (x, y, z) = request.position;
            // SAFETY: world_ctx is a live actor and the class
            // came from this frame's search; game thread.
            // Collision checking off, so a blocked spot does not
            // silently swallow the spawn.
            let actor = unsafe {
                ueforge::ue::spawn::spawn_ai_from_class(
                    world_ctx,
                    request.class.ptr,
                    (x, y, z),
                    0.0,
                    true,
                )
            };
            actor != 0
        },
    );

    ueforge::log::log(format_args!(
        "spawning: {} spawned {spawned} extra NPC(s)",
        ueforge::ue::actor::short_name(&plan.place)
    ));
    Ok(serde_json::json!({"spawned": spawned}))
}

/// Adds adaptive-spawning status and override commands to the MISERY debug API.
/// Stays here because the controls expose this mod's enemy policy, not a shared spawning primitive.
fn register_ops() {
    ueforge::ops::OP_REGISTRY.register_many([
        ueforge::ops::OpDef::new(
            "spawning_stats",
            "Scaling spawner counters",
            "{}",
            |_args| {
                let mut guard = PLANNER.lock().unwrap_or_else(|e| e.into_inner());
                let planner = guard.get_or_insert_with(|| EncounterPlanner::new(ENCOUNTER_CONFIG));
                Ok(serde_json::json!({
                    "spawned_total": planner.spawned_total(),
                    "squares_processed": planner.processed_total(),
                    "emissions": LAST_EMISSIONS.load(Ordering::Relaxed),
                    "override": EMISSIONS_OVERRIDE.load(Ordering::Relaxed),
                }))
            },
        ),
        ueforge::ops::OpDef::new(
            "spawning_override",
            "Force the emission level for testing (-1 = live value)",
            "{emissions: i64}",
            |args| {
                let v = args.get("emissions").and_then(|v| v.as_i64()).unwrap_or(-1);
                EMISSIONS_OVERRIDE.store(v as i32, Ordering::Relaxed);
                Ok(serde_json::json!({"override": v}))
            },
        ),
    ]);
}
