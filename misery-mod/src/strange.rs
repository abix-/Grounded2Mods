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
use std::time::Duration;

use modforge::storyteller::{
    PhenomenonConfig, PhenomenonDef, PhenomenonNature as Nature, PhenomenonPlanner,
};
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
const SESSION_CAP: usize = 400;
/// Keep phenomena off the square's very edge (units).
const EDGE_MARGIN: f64 = 1200.0;
const POLL: Duration = Duration::from_secs(5);

// Generator fields (worldgen.md 2).
const TILE_SIZE_OFFSET: usize = 0x2C0;
const STREAMING_LEVELS_OFFSET: usize = 0x2E8;
const EMISSIONS_PAST_OFFSET: usize = 0x2F8;

static PLANNER: std::sync::Mutex<Option<PhenomenonPlanner<String>>> = std::sync::Mutex::new(None);

/// One kind of phenomenon: what to spawn, how many, how tightly
/// clustered, and how its odds move with the shining count.
struct Phenomenon {
    name: &'static str,
    /// Classes to draw from; each prop picks one at random.
    classes: &'static [&'static str],
    planning: PhenomenonDef,
}

/// The catalog. Classes come from the game's own actors
/// (research: anomaly, container, and camp Blueprints).
static PHENOMENA: &[Phenomenon] = &[
    Phenomenon {
        name: "anomaly_field",
        classes: &["BP_AnomalyZone_C", "BP_AnomalyCluster_C"],
        planning: PhenomenonDef {
            count_min: 3,
            count_max: 7,
            spread: 1800.0,
            weight_base: 1.0,
            weight_per_level: 0.04,
            nature: Nature::Danger,
        },
    },
    Phenomenon {
        name: "artifact_seam",
        classes: &["BP_ArtifactSpawner_C"],
        planning: PhenomenonDef {
            count_min: 2,
            count_max: 5,
            spread: 1200.0,
            weight_base: 0.8,
            weight_per_level: 0.01,
            nature: Nature::Reward,
        },
    },
    Phenomenon {
        name: "wandering_lights",
        classes: &["BP_WanderingLights_C", "BP_WanderingLightOrigin_C"],
        planning: PhenomenonDef {
            count_min: 2,
            count_max: 5,
            spread: 2500.0,
            weight_base: 1.0,
            weight_per_level: 0.0,
            nature: Nature::Neutral,
        },
    },
    Phenomenon {
        name: "teleport_nest",
        classes: &["BP_TeleportAnomaly_C"],
        planning: PhenomenonDef {
            count_min: 2,
            count_max: 4,
            spread: 1500.0,
            weight_base: 0.3,
            weight_per_level: 0.03,
            nature: Nature::Danger,
        },
    },
    Phenomenon {
        name: "trampoline_garden",
        classes: &["BP_Tramplin_Anomaly_C"],
        planning: PhenomenonDef {
            count_min: 4,
            count_max: 9,
            spread: 2200.0,
            weight_base: 0.4,
            weight_per_level: 0.01,
            nature: Nature::Neutral,
        },
    },
    Phenomenon {
        name: "garbage_drift",
        classes: &["BP_AnomalyGarbage_C"],
        planning: PhenomenonDef {
            count_min: 3,
            count_max: 8,
            spread: 2000.0,
            weight_base: 0.6,
            weight_per_level: 0.0,
            nature: Nature::Neutral,
        },
    },
    Phenomenon {
        name: "hedge_maze",
        classes: &["BP_Hedge_C"],
        planning: PhenomenonDef {
            count_min: 5,
            count_max: 12,
            spread: 2000.0,
            weight_base: 0.5,
            weight_per_level: 0.0,
            nature: Nature::Neutral,
        },
    },
    Phenomenon {
        name: "floating_debris",
        classes: &["BP_FloatingMesh_C"],
        planning: PhenomenonDef {
            count_min: 3,
            count_max: 8,
            spread: 2000.0,
            weight_base: 0.5,
            weight_per_level: 0.01,
            nature: Nature::Neutral,
        },
    },
    Phenomenon {
        name: "abandoned_camp",
        classes: &[
            "BP_Tent_Start_C",
            "BP_WoodenBoxResource_C",
            "BP_StashStart_C",
        ],
        planning: PhenomenonDef {
            count_min: 3,
            count_max: 6,
            spread: 900.0,
            weight_base: 1.0,
            weight_per_level: -0.01,
            nature: Nature::Reward,
        },
    },
    Phenomenon {
        name: "supply_cache",
        classes: &[
            "BP_GradBigCrate_C",
            "BP_WoodenBoxResource_C",
            "BP_AirCrate_C",
        ],
        planning: PhenomenonDef {
            count_min: 2,
            count_max: 5,
            spread: 700.0,
            weight_base: 0.9,
            weight_per_level: -0.01,
            nature: Nature::Reward,
        },
    },
    Phenomenon {
        name: "black_hole",
        classes: &["BP_BlackHole_C"],
        planning: PhenomenonDef {
            count_min: 1,
            count_max: 1,
            spread: 0.0,
            weight_base: 0.08,
            weight_per_level: 0.012,
            nature: Nature::Danger,
        },
    },
];

/// Emission level: max EmissionsPast across generators
/// (worldgen.md 7.1: it is the save's global shining count).
/// Reads the world's Shining count to scale alternate-reality phenomena.
/// Stays here because the progression field lives on MISERY's world-generation Blueprint.
fn emission_level() -> i32 {
    ueforge::ue::actor::find_actors_by_chain("BP_WorldGeneration_Base_C")
        .into_iter()
        .map(|p| unsafe { read_at::<i32>(p, EMISSIONS_PAST_OFFSET) })
        .max()
        .unwrap_or(0)
}

/// TileSize of the generator that is currently streaming a world.
/// Finds the current MISERY map tile size used to place phenomena inside a square.
/// Stays here because the world-generator class and tile-size field are game-specific.
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
/// Decodes a MISERY actor path into its world name and square coordinates.
/// Stays here because the coordinate naming convention belongs to this game's streamed maps.
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
/// Lists the distinct MISERY map squares currently populated by live creatures.
/// Stays here because it identifies squares through this game's NPC classes and package names.
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

/// Starts the alternate-reality overlay that adds phenomena to newly loaded squares.
/// Stays here because it connects Modforge planning to MISERY polling and Unreal spawning.
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

/// Notices each newly loaded square and schedules its phenomena on the game thread.
/// Stays here because live-square discovery and game-thread dispatch are MISERY integration.
fn watcher() {
    if ue::try_runtime().is_none() {
        return;
    }
    {
        let mut guard = PLANNER.lock().unwrap_or_else(|e| e.into_inner());
        let planner = guard.get_or_insert_with(|| PhenomenonPlanner::new(PHENOMENON_CONFIG));
        if planner.is_full() {
            return;
        }
    }
    let squares = live_squares();
    let live: HashSet<String> = squares.iter().map(|(name, _, _)| name.clone()).collect();
    {
        let mut guard = PLANNER.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .get_or_insert_with(|| PhenomenonPlanner::new(PHENOMENON_CONFIG))
            .retain_places(|place| live.contains(place));
    }

    let Some(tile) = active_tile_size() else {
        return;
    };
    let emissions = emission_level();
    let defs: Vec<PhenomenonDef> = PHENOMENA.iter().map(|p| p.planning).collect();

    for (name, cx, cy) in squares {
        let plan = {
            let mut guard = PLANNER.lock().unwrap_or_else(|e| e.into_inner());
            guard
                .get_or_insert_with(|| PhenomenonPlanner::new(PHENOMENON_CONFIG))
                .plan_place(name.clone(), emissions as f64, &defs)
        };
        let Some(plan) = plan else {
            continue;
        };
        if plan.phenomena.is_empty() {
            ueforge::log::log(format_args!("strange: {name} stays mundane"));
            continue;
        }
        let labels: Vec<&str> = plan
            .phenomena
            .iter()
            .map(|&index| PHENOMENA[index].name)
            .collect();
        ueforge::log::log(format_args!(
            "strange: {name} rolls {labels:?} (emissions {emissions})"
        ));
        let centre = (cx as f64 * tile, cy as f64 * tile);
        let half = tile / 2.0 - EDGE_MARGIN;
        // `run`, not `enqueue`: this watcher already runs ON
        // the game thread, and queueing from there waits for
        // a drain that cannot start until we return.
        let r = ueforge::game_thread::run(
            move || place_phenomena(&plan.phenomena, centre, half),
            Duration::from_secs(15),
        );
        if let Err(e) = r {
            ueforge::log::log(format_args!("strange: placement failed: {e}"));
        }
    }
}

/// MISERY's tuning values for Modforge's phenomenon planner.
const PHENOMENON_CONFIG: PhenomenonConfig = PhenomenonConfig {
    budget: modforge::roll::Budget {
        quiet_chance: QUIET_CHANCE,
        at_zero: PHENOMENA_AT_ZERO,
        per_level: PHENOMENA_PER_EMISSION,
        intensity: INTENSITY,
        max: PHENOMENA_CAP,
    },
    session_cap: SESSION_CAP,
};

/// Game thread. Place each phenomenon at its own random point in
/// the square, props scattered within its spread.
/// Stays here because Blueprint lookup, ground traces, and Unreal spawning are MISERY integration.
fn place_phenomena(
    plan: &[usize],
    centre: (f64, f64),
    half: f64,
) -> Result<serde_json::Value, String> {
    let world_ctx = ueforge::ue::actor::find_actors_by_chain("BP_SGKMasterCharacter_C")
        .into_iter()
        .next()
        .ok_or("no player for world context")?;

    let defs: Vec<PhenomenonDef> = PHENOMENA.iter().map(|p| p.planning).collect();
    let mut guard = PLANNER.lock().unwrap_or_else(|e| e.into_inner());
    let planner = guard.get_or_insert_with(|| PhenomenonPlanner::new(PHENOMENON_CONFIG));
    let placed = planner.execute(
        plan,
        &defs,
        centre,
        half,
        |x, y| {
            // SAFETY: world_ctx is a live actor, game thread.
            unsafe {
                ueforge::ue::trace::ground_z(world_ctx, x, y, crate::TRACE_UP, crate::TRACE_DOWN)
            }
        },
        |index| PHENOMENA[index].classes.len(),
        |index, variant| {
            let class_name = PHENOMENA[index].classes[variant];
            ue::find_class_fast(class_name).map(|class| class.as_object().as_ptr() as u64)
        },
        |request| {
            // SAFETY: world_ctx is a live actor and the class came
            // from this frame's lookup; game thread.
            let actor = unsafe {
                ueforge::ue::spawn::spawn_actor(
                    world_ctx,
                    request.class,
                    request.position,
                    request.yaw,
                    1.0,
                )
            };
            actor != 0
        },
    );
    ueforge::log::log(format_args!("strange: placed {placed} prop(s)"));
    Ok(serde_json::json!({"placed": placed}))
}

/// Adds phenomenon status and manual placement controls to the MISERY debug API.
/// Stays here because these operations expose this mod's alternate-reality content.
fn register_ops() {
    ueforge::ops::OP_REGISTRY.register_many([
        ueforge::ops::OpDef::new(
            "strange_stats",
            "Alternate-reality overlay counters",
            "{}",
            |_a| {
                let mut guard = PLANNER.lock().unwrap_or_else(|e| e.into_inner());
                let planner =
                    guard.get_or_insert_with(|| PhenomenonPlanner::new(PHENOMENON_CONFIG));
                Ok(serde_json::json!({
                    "props_spawned": planner.spawned_total(),
                    "phenomena_placed": planner.phenomena_total(),
                    "emissions": emission_level(),
                    "tile_size": active_tile_size(),
                }))
            },
        ),
        ueforge::ops::OpDef::new(
            "strange_here",
            "Place one named phenomenon at the player (testing)",
            "{name: str}",
            |args| {
                let want = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let index = PHENOMENA
                    .iter()
                    .position(|p| p.name == want)
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
                        place_phenomena(&[index], (loc.0, loc.1), 400.0)
                    },
                    Duration::from_secs(15),
                )
            },
        ),
    ]);
}
