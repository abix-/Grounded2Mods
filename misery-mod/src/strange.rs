//! Alternate-reality overlays: phenomena the author never placed.
//!
//! Every map square that streams in rolls for phenomena: anomaly
//! fields, teleport nests, black holes, abandoned camps, caches,
//! wandering lights, and stranger things. Placement is anywhere
//! inside the square's footprint (worldgen.md 4.2: a square's
//! centre is cell * TileSize, extending half a tile each way),
//! dropped onto the ground with a line trace.
//!
//! The point is surprise: knowing the algorithm should not let
//! you predict a world. Rolls are per square per stream-in,
//! weights shift with the save's shining count, and the rarest
//! phenomena are rare enough to stay stories.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use ueforge::ue::{self, UObject, read_at};

use crate::dispatch;

/// Overall intensity; multiplies how many phenomena a square rolls.
const INTENSITY: f64 = 1.0;
/// Chance a square stays mundane, whatever the emission level.
const QUIET_CHANCE: f64 = 0.35;
/// Phenomena per square: base plus emission-scaled bonus.
const PHENOMENA_AT_ZERO: f64 = 0.6;
const PHENOMENA_PER_EMISSION: f64 = 0.03;
const PHENOMENA_CAP: usize = 4;
/// Session cap on spawned props.
const SESSION_CAP: u64 = 400;
/// Keep phenomena off the square's very edge (units).
const EDGE_MARGIN: f64 = 1200.0;
const POLL: Duration = Duration::from_secs(5);

// Generator fields (worldgen.md 2).
const TILE_SIZE_OFFSET: usize = 0x2C0;
const STREAMING_LEVELS_OFFSET: usize = 0x2E8;
const EMISSIONS_PAST_OFFSET: usize = 0x2F8;

static SPAWNED_TOTAL: AtomicU64 = AtomicU64::new(0);
static PHENOMENA_TOTAL: AtomicU64 = AtomicU64::new(0);

/// What a phenomenon does to the player: worth taking, worth
/// fearing, or neither. Rewards get a danger placed on top of
/// them so the prize is visibly guarded: the feeling wanted is
/// excited AND scared, not one or the other.
#[derive(PartialEq, Clone, Copy)]
enum Nature {
    Reward,
    Danger,
    Neutral,
}

/// One kind of phenomenon: what to spawn, how many, how tightly
/// clustered, and how its odds move with the shining count.
struct Phenomenon {
    name: &'static str,
    /// Classes to draw from; each prop picks one at random.
    classes: &'static [&'static str],
    /// Prop count range.
    count: (usize, usize),
    /// Cluster radius in world units.
    spread: f64,
    /// Selection weight at emission 0.
    weight_base: f64,
    /// Weight added per emission (can be negative for early-game
    /// phenomena that fade as the world gets stranger).
    weight_per_emission: f64,
    nature: Nature,
}

/// The catalog. Classes come from the game's own actors
/// (research: anomaly, container, and camp Blueprints).
static PHENOMENA: &[Phenomenon] = &[
    Phenomenon {
        name: "anomaly_field",
        classes: &["BP_AnomalyZone_C", "BP_AnomalyCluster_C"],
        count: (3, 7),
        spread: 1800.0,
        weight_base: 1.0,
        weight_per_emission: 0.04,
        nature: Nature::Danger,
    },
    Phenomenon {
        name: "artifact_seam",
        classes: &["BP_ArtifactSpawner_C"],
        count: (2, 5),
        spread: 1200.0,
        weight_base: 0.8,
        weight_per_emission: 0.01,
        nature: Nature::Reward,
    },
    Phenomenon {
        name: "wandering_lights",
        classes: &["BP_WanderingLights_C", "BP_WanderingLightOrigin_C"],
        count: (2, 5),
        spread: 2500.0,
        weight_base: 1.0,
        weight_per_emission: 0.0,
        nature: Nature::Neutral,
    },
    Phenomenon {
        name: "teleport_nest",
        classes: &["BP_TeleportAnomaly_C"],
        count: (2, 4),
        spread: 1500.0,
        weight_base: 0.3,
        weight_per_emission: 0.03,
        nature: Nature::Danger,
    },
    Phenomenon {
        name: "trampoline_garden",
        classes: &["BP_Tramplin_Anomaly_C"],
        count: (4, 9),
        spread: 2200.0,
        weight_base: 0.4,
        weight_per_emission: 0.01,
        nature: Nature::Neutral,
    },
    Phenomenon {
        name: "garbage_drift",
        classes: &["BP_AnomalyGarbage_C"],
        count: (3, 8),
        spread: 2000.0,
        weight_base: 0.6,
        weight_per_emission: 0.0,
        nature: Nature::Neutral,
    },
    Phenomenon {
        name: "hedge_maze",
        classes: &["BP_Hedge_C"],
        count: (5, 12),
        spread: 2000.0,
        weight_base: 0.5,
        weight_per_emission: 0.0,
        nature: Nature::Neutral,
    },
    Phenomenon {
        name: "floating_debris",
        classes: &["BP_FloatingMesh_C"],
        count: (3, 8),
        spread: 2000.0,
        weight_base: 0.5,
        weight_per_emission: 0.01,
        nature: Nature::Neutral,
    },
    Phenomenon {
        name: "abandoned_camp",
        classes: &["BP_Tent_Start_C", "BP_WoodenBoxResource_C", "BP_StashStart_C"],
        count: (3, 6),
        spread: 900.0,
        weight_base: 1.0,
        weight_per_emission: -0.01,
        nature: Nature::Reward,
    },
    Phenomenon {
        name: "supply_cache",
        classes: &["BP_GradBigCrate_C", "BP_WoodenBoxResource_C", "BP_AirCrate_C"],
        count: (2, 5),
        spread: 700.0,
        weight_base: 0.9,
        weight_per_emission: -0.01,
        nature: Nature::Reward,
    },
    Phenomenon {
        name: "black_hole",
        classes: &["BP_BlackHole_C"],
        count: (1, 1),
        spread: 0.0,
        weight_base: 0.08,
        weight_per_emission: 0.012,
        nature: Nature::Danger,
    },
];

/// Emission level: max EmissionsPast across generators
/// (worldgen.md 7.1: it is the save's global shining count).
fn emission_level() -> i32 {
    ueforge::ue::actor::find_actors_by_chain("BP_WorldGeneration_Base_C")
        .into_iter()
        .map(|p| unsafe { read_at::<i32>(p, EMISSIONS_PAST_OFFSET) })
        .max()
        .unwrap_or(0)
}

/// TileSize of the generator that is currently streaming a world.
pub fn active_tile_size() -> Option<f64> {
    for p in ueforge::ue::actor::find_actors_by_chain("BP_WorldGeneration_Base_C") {
        let streaming_num: i32 = unsafe { read_at(p, STREAMING_LEVELS_OFFSET + 8) };
        if streaming_num > 0 {
            let tile: f64 = unsafe { read_at(p, TILE_SIZE_OFFSET) };
            if tile > 0.0 {
                return Some(tile);
            }
        }
    }
    None
}

/// Square key plus its grid cell, from an actor's full name
/// (`Class /Game/.../<worldid>_<cx>_<cy>.L_Preset.PersistentLevel...`).
fn square_of(full_name: &str) -> Option<(String, i32, i32)> {
    let path = full_name.split(' ').nth(1)?;
    let square = path.split(".PersistentLevel").next()?;
    let short = square.rsplit('/').next()?;
    let grid = short.split('.').next()?;
    let mut parts = grid.split('_');
    let _world = parts.next()?;
    let cx = parts.next()?.parse().ok()?;
    let cy = parts.next()?.parse().ok()?;
    Some((short.to_string(), cx, cy))
}

/// Live squares, keyed by name, with their grid cells.
pub fn live_squares() -> Vec<(String, i32, i32)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for p in ueforge::ue::actor::find_actors_by_chain("BP_MasterAICharacter_C") {
        // SAFETY: p came from this call's GObjects iteration.
        let obj = unsafe { &*(p as *const UObject) };
        let full = obj.full_name();
        if !full.contains("WorldPresets") {
            continue;
        }
        if let Some((name, cx, cy)) = square_of(&full) {
            if seen.insert(name.clone()) {
                out.push((name, cx, cy));
            }
        }
    }
    out
}

pub fn install() {
    register_ops();
    // Stoppable so the DLL can unload cleanly on a hot reload.
    std::mem::forget(modforge::rpg::poller::spawn_interval(
        "misery-strange",
        POLL,
        // Reads live actors, so it runs on the game thread.
        ueforge::game_thread::each_tick(watcher),
    ));
}

/// Squares already rolled for phenomena.
static DONE: std::sync::Mutex<Option<HashSet<String>>> = std::sync::Mutex::new(None);

fn watcher() {
    let mut guard = DONE.lock().unwrap_or_else(|e| e.into_inner());
    let done = guard.get_or_insert_with(HashSet::new);
    {
        if ue::try_runtime().is_none() {
            return;
        }
        if SPAWNED_TOTAL.load(Ordering::Relaxed) >= SESSION_CAP {
            return;
        }
        let squares = live_squares();
        let live: HashSet<String> = squares.iter().map(|(n, _, _)| n.clone()).collect();
        // A square that unloaded rolls fresh when it returns.
        done.retain(|s| live.contains(s));

        let Some(tile) = active_tile_size() else { return };
        let emissions = emission_level();

        for (name, cx, cy) in squares {
            if done.contains(&name) {
                continue;
            }
            done.insert(name.clone());
            let plan = roll_square(emissions);
            if plan.is_empty() {
                ueforge::log::log(format_args!("strange: {name} stays mundane"));
                continue;
            }
            let labels: Vec<&str> = plan.iter().map(|p| p.name).collect();
            ueforge::log::log(format_args!(
                "strange: {name} rolls {labels:?} (emissions {emissions})"
            ));
            let centre = (cx as f64 * tile, cy as f64 * tile);
            let half = tile / 2.0 - EDGE_MARGIN;
            // `run`, not `enqueue`: this watcher already runs ON
            // the game thread, and queueing from there waits for
            // a drain that cannot start until we return.
            let r = ueforge::game_thread::run(
                move || place_phenomena(&plan, centre, half),
                Duration::from_secs(15),
            );
            if let Err(e) = r {
                ueforge::log::log(format_args!("strange: placement failed: {e}"));
            }
        }
    }
}

/// Roll which phenomena a square gets. Weighted draw without
/// replacement so one square rarely doubles a phenomenon.
/// How many phenomena a square gets. The curve, the quiet
/// chance, the cap and the weighted picking are
/// `modforge::roll`, which is unit-tested; what stays here is
/// the phenomena themselves and the reward-needs-a-danger rule.
const BUDGET: modforge::roll::Budget = modforge::roll::Budget {
    quiet_chance: QUIET_CHANCE,
    at_zero: PHENOMENA_AT_ZERO,
    per_level: PHENOMENA_PER_EMISSION,
    intensity: INTENSITY,
    max: PHENOMENA_CAP,
};

fn roll_square(emissions: i32) -> Vec<&'static Phenomenon> {
    if BUDGET.is_quiet() {
        return Vec::new();
    }
    let level = emissions as f64;
    let count = BUDGET.roll_count(level);
    let weights: Vec<modforge::roll::Weight> = PHENOMENA
        .iter()
        .map(|p| modforge::roll::Weight::new(p.weight_base, p.weight_per_emission))
        .collect();
    let mut chosen: Vec<&'static Phenomenon> = modforge::roll::pick_distinct(&weights, level, count)
        .into_iter()
        .map(|i| &PHENOMENA[i])
        .collect();

    // A reward with nothing guarding it is just a gift. If the
    // square rolled something worth taking but nothing worth
    // fearing, draw a danger to sit on top of it.
    let has_reward = chosen.iter().any(|p| p.nature == Nature::Reward);
    let has_danger = chosen.iter().any(|p| p.nature == Nature::Danger);
    if has_reward && !has_danger {
        let dangers: Vec<&'static Phenomenon> =
            PHENOMENA.iter().filter(|p| p.nature == Nature::Danger).collect();
        if !dangers.is_empty() {
            chosen.push(dangers[fastrand::usize(0..dangers.len())]);
        }
    }
    chosen
}

/// Game thread. Place each phenomenon at its own random point in
/// the square, props scattered within its spread.
fn place_phenomena(
    plan: &[&'static Phenomenon],
    centre: (f64, f64),
    half: f64,
) -> Result<serde_json::Value, String> {
    let world_ctx = ueforge::ue::actor::find_actors_by_chain("BP_SGKMasterCharacter_C")
        .into_iter()
        .next()
        .ok_or("no player for world context")?;

    // Rewards are placed first so a danger drawn after them can
    // land on the prize instead of somewhere harmless.
    let mut ordered: Vec<&'static Phenomenon> = plan.to_vec();
    ordered.sort_by_key(|p| match p.nature {
        Nature::Reward => 0,
        Nature::Danger => 1,
        Nature::Neutral => 2,
    });

    // Where the square's reward sits, so its guard can be placed
    // on the same spot rather than somewhere harmless.
    let mut reward_spot: Option<(f64, f64)> = None;
    let mut placed = 0usize;
    for phenomenon in &ordered {
        let (px, py) = match (phenomenon.nature, reward_spot) {
            // The guard lands on the prize: you see both at once.
            (Nature::Danger, Some(spot)) => spot,
            _ => (
                centre.0 + (fastrand::f64() * 2.0 - 1.0) * half,
                centre.1 + (fastrand::f64() * 2.0 - 1.0) * half,
            ),
        };
        if phenomenon.nature == Nature::Reward && reward_spot.is_none() {
            reward_spot = Some((px, py));
        }
        let n = phenomenon.count.0
            + fastrand::usize(0..=(phenomenon.count.1 - phenomenon.count.0));
        for _ in 0..n {
            if SPAWNED_TOTAL.load(Ordering::Relaxed) >= SESSION_CAP {
                break;
            }
            let ang = fastrand::f64() * std::f64::consts::TAU;
            let dist = fastrand::f64() * phenomenon.spread;
            let x = px + ang.cos() * dist;
            let y = py + ang.sin() * dist;
            // SAFETY: world_ctx is a live actor, game thread.
            let Some(z) = (unsafe {
                ueforge::ue::trace::ground_z(world_ctx, x, y, crate::TRACE_UP, crate::TRACE_DOWN)
            }) else {
                continue;
            };
            let class_name =
                phenomenon.classes[fastrand::usize(0..phenomenon.classes.len())];
            let Some(class) = ue::find_class_fast(class_name) else { continue };
            let yaw = fastrand::f64() * std::f64::consts::TAU;
            // SAFETY: world_ctx is a live actor and the class came
            // from this frame's lookup; game thread.
            let actor = unsafe {
                ueforge::ue::spawn::spawn_actor(
                    world_ctx,
                    class.as_object().as_ptr() as u64,
                    (x, y, z),
                    yaw,
                    1.0,
                )
            };
            if actor != 0 {
                SPAWNED_TOTAL.fetch_add(1, Ordering::Relaxed);
                placed += 1;
            }
        }
        PHENOMENA_TOTAL.fetch_add(1, Ordering::Relaxed);
    }
    ueforge::log::log(format_args!("strange: placed {placed} prop(s)"));
    Ok(serde_json::json!({"placed": placed}))
}


fn register_ops() {
    ueforge::ops::OP_REGISTRY.register_many([
        ueforge::ops::OpDef::new("strange_stats", "Alternate-reality overlay counters", "{}", |_a| {
            Ok(serde_json::json!({
                "props_spawned": SPAWNED_TOTAL.load(Ordering::Relaxed),
                "phenomena_placed": PHENOMENA_TOTAL.load(Ordering::Relaxed),
                "emissions": emission_level(),
                "tile_size": active_tile_size(),
            }))
        }),
        ueforge::ops::OpDef::new(
            "strange_here",
            "Place one named phenomenon at the player (testing)",
            "{name: str}",
            |args| {
                let want = args.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let p = PHENOMENA
                    .iter()
                    .find(|p| p.name == want)
                    .ok_or_else(|| format!("unknown phenomenon '{want}'"))?;
                dispatch::DRAIN.queue().enqueue(
                    move || {
                        let player =
                            ueforge::ue::actor::find_actors_by_chain("BP_SGKMasterCharacter_C")
                                .into_iter()
                                .next()
                                .ok_or("no player")?;
                        // SAFETY: live player actor, game thread.
                        let loc = unsafe { ueforge::ue::transform::world_location(player) }
                            .ok_or("no player location")?;
                        place_phenomena(&[p], (loc.0, loc.1), 400.0)
                    },
                    Duration::from_secs(15),
                )
            },
        ),
    ]);
}

