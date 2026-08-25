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

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::time::Duration;

use ueforge::ue::{self, UObject, read_at};

use crate::dispatch;

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

/// Classes never used as escalations (harmless or friendly).
const NO_ESCALATE: &[&str] = &["Tamed", "DeerNeutral", "Boar"];

static SPAWNED_TOTAL: AtomicU64 = AtomicU64::new(0);
static SQUARES_PROCESSED: AtomicU64 = AtomicU64::new(0);
static LAST_EMISSIONS: AtomicI32 = AtomicI32::new(-1);
/// Test override for the emission level; negative = use live.
static EMISSIONS_OVERRIDE: AtomicI32 = AtomicI32::new(-1);

/// What one square rolls; executed as one game-thread job.
struct Plan {
    square: String,
    copies: usize,
    escalations: usize,
    pack: usize,
}

/// The save's shining count: max EmissionsPast across the four
/// world generators (only the active one accumulates).
fn emission_level() -> i32 {
    let ov = EMISSIONS_OVERRIDE.load(Ordering::Relaxed);
    if ov >= 0 {
        return ov;
    }
    ueforge::ue::actor::find_actors_by_chain("BP_WorldGeneration_Base_C")
        .into_iter()
        .map(|p| unsafe { read_at::<i32>(p, EMISSIONS_PAST_OFFSET) })
        .max()
        .unwrap_or(0)
}

/// Live hostile NPCs grouped by the map square that owns them.
/// Squares are identified by the level path before
/// ".PersistentLevel"; only WorldPresets squares count (the hub
/// and anything we spawned live elsewhere and are excluded).
fn census() -> HashMap<String, usize> {
    let mut squares: HashMap<String, usize> = HashMap::new();
    for p in ueforge::ue::actor::find_actors_by_chain("BP_MasterAICharacter_C") {
        // SAFETY: p came from the GObjects iteration this call.
        let obj = unsafe { &*(p as *const UObject) };
        if let Some(square) = square_of(&obj.full_name()) {
            *squares.entry(square).or_default() += 1;
        }
    }
    squares
}

/// The map square that owns an actor, from its full name.
/// Full names are "ClassName /Game/path/Grid.Level.PersistentLevel.Actor";
/// the class prefix MUST be stripped or every class becomes its
/// own square key.
fn square_of(full_name: &str) -> Option<String> {
    let path = full_name.split(' ').nth(1)?;
    if !path.contains("WorldPresets") {
        return None;
    }
    Some(path.split(".PersistentLevel").next()?.to_string())
}

fn roll_plan(square: &str, vanilla: usize, emissions: i32) -> Plan {
    let mut plan = Plan {
        square: square.to_string(),
        copies: 0,
        escalations: 0,
        pack: 0,
    };
    if fastrand::f64() < QUIET_CHANCE {
        return plan;
    }
    let f = INTENSITY * (emissions as f64) / DOUBLE_AT_EMISSIONS;
    let r = fastrand::f64() * 2.0 * f;
    let extras = ((vanilla as f64) * r).round() as usize;
    let extras = extras.min(PER_SQUARE_CAP);

    let p_escalate =
        (ESCALATE_BASE + ESCALATE_PER_EMISSION * emissions as f64).min(ESCALATE_CAP);
    for _ in 0..extras {
        if fastrand::f64() < p_escalate {
            plan.escalations += 1;
        } else {
            plan.copies += 1;
        }
    }

    let p_pack = (PACK_BASE + PACK_PER_EMISSION * emissions as f64).min(PACK_CAP);
    if fastrand::f64() < p_pack {
        plan.pack = 3 + fastrand::usize(0..=2);
    }
    plan
}

/// Background watcher. Rolls a plan for each newly streamed
/// square and executes it on the game thread.
pub fn install() {
    register_ops();
    let _ = std::thread::Builder::new()
        .name("misery-spawning".into())
        .spawn(watcher);
}

fn watcher() {
    let mut processed: HashSet<String> = HashSet::new();
    loop {
        std::thread::sleep(POLL);
        if ue::try_runtime().is_none() {
            continue;
        }
        let squares = census();
        // A square that unloaded rolls fresh next time it streams in.
        processed.retain(|s| squares.contains_key(s));

        if SPAWNED_TOTAL.load(Ordering::Relaxed) >= SESSION_CAP {
            continue;
        }
        let emissions = emission_level();
        LAST_EMISSIONS.store(emissions, Ordering::Relaxed);

        for (square, vanilla) in &squares {
            if processed.contains(square) {
                continue;
            }
            processed.insert(square.clone());
            SQUARES_PROCESSED.fetch_add(1, Ordering::Relaxed);
            let plan = roll_plan(square, *vanilla, emissions);
            let total = plan.copies + plan.escalations + plan.pack;
            ueforge::log::log(format_args!(
                "spawning: {} vanilla={vanilla} emissions={emissions} copies={} escalations={} pack={}",
                short(square), plan.copies, plan.escalations, plan.pack,
            ));
            if total == 0 {
                continue;
            }
            let r = dispatch::DRAIN.queue().enqueue(
                move || execute_plan(&plan),
                Duration::from_secs(10),
            );
            if let Err(e) = r {
                ueforge::log::log(format_args!("spawning: plan failed: {e}"));
            }
        }
    }
}

fn short(square: &str) -> &str {
    square.rsplit('/').next().unwrap_or(square)
}

/// Runs on the game thread. Re-censuses the square (pointers
/// from the watcher could be stale), then spawns.
fn execute_plan(plan: &Plan) -> Result<serde_json::Value, String> {
    let mut anchors: Vec<*const u8> = Vec::new();
    let mut pool: Vec<(u64, String)> = Vec::new();
    for p in ueforge::ue::actor::find_actors_by_chain("BP_MasterAICharacter_C") {
        // SAFETY: p is live; we are on the game thread, nothing
        // streams out mid-frame.
        let obj = unsafe { &*(p as *const UObject) };
        let class_name = obj.class().map(|c| c.as_object().name()).unwrap_or_default();
        let class_ptr = unsafe { read_at::<u64>(p, 0x10) };
        if class_ptr != 0 && !NO_ESCALATE.iter().any(|n| class_name.contains(n)) {
            pool.push((class_ptr, class_name));
        }
        if square_of(&obj.full_name()).as_deref() == Some(plan.square.as_str()) {
            anchors.push(p);
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

    let mut spawned = 0usize;
    let mut spawn_near = |anchor: *const u8, class_ptr: u64| {
        if SPAWNED_TOTAL.load(Ordering::Relaxed) >= SESSION_CAP {
            return;
        }
        if let Some((x, y, z)) = actor_location(anchor) {
            let ang = fastrand::f64() * std::f64::consts::TAU;
            let dist = JITTER_MIN + fastrand::f64() * (JITTER_MAX - JITTER_MIN);
            let (sx, sy) = (x + ang.cos() * dist, y + ang.sin() * dist);
            if spawn_class(world_ctx, class_ptr, sx, sy, z + 100.0) != 0 {
                SPAWNED_TOTAL.fetch_add(1, Ordering::Relaxed);
                spawned += 1;
            }
        }
    };

    for _ in 0..plan.copies {
        let anchor = anchors[fastrand::usize(0..anchors.len())];
        let class_ptr = unsafe { read_at::<u64>(anchor, 0x10) };
        if class_ptr != 0 {
            spawn_near(anchor, class_ptr);
        }
    }
    for _ in 0..plan.escalations {
        let anchor = anchors[fastrand::usize(0..anchors.len())];
        let (class_ptr, _) = pool[fastrand::usize(0..pool.len())];
        spawn_near(anchor, class_ptr);
    }
    if plan.pack > 0 {
        let anchor = anchors[fastrand::usize(0..anchors.len())];
        let (class_ptr, name) = pool[fastrand::usize(0..pool.len())].clone();
        ueforge::log::log(format_args!(
            "spawning: PACK of {} x {name} at {}",
            plan.pack,
            short(&plan.square)
        ));
        for _ in 0..plan.pack {
            spawn_near(anchor, class_ptr);
        }
    }

    ueforge::log::log(format_args!(
        "spawning: {} spawned {spawned} extra NPC(s)",
        short(&plan.square)
    ));
    Ok(serde_json::json!({"spawned": spawned}))
}

/// Actor:K2_GetActorLocation via ProcessEvent. Game thread only.
fn actor_location(actor: *const u8) -> Option<(f64, f64, f64)> {
    let cls = ue::find_class_fast("Actor")?;
    let func = cls.get_function("Actor", "K2_GetActorLocation")?;
    let mut parms = [0f64; 3];
    // SAFETY: actor is a live UObject on the game thread; parms
    // matches the UFunction's 0x18-byte return layout.
    unsafe {
        (*(actor as *const UObject)).process_event(func, parms.as_mut_ptr() as *mut c_void);
    }
    Some((parms[0], parms[1], parms[2]))
}

/// AIBlueprintHelperLibrary:SpawnAIFromClass via ProcessEvent.
/// Parm layout from the object dump (research.md 26.3). Game
/// thread only. Returns the spawned pawn address or 0.
fn spawn_class(world_ctx: *const u8, class_ptr: u64, x: f64, y: f64, z: f64) -> u64 {
    #[repr(C)]
    struct Parms {
        world_context: u64,
        pawn_class: u64,
        behavior_tree: u64,
        location: [f64; 3],
        rotation: [f64; 3],
        b_no_collision_fail: u8,
        _pad: [u8; 7],
        owner: u64,
        return_value: u64,
    }
    let Some(cls) = ue::find_class_fast("AIBlueprintHelperLibrary") else {
        return 0;
    };
    let Some(func) = cls.get_function("AIBlueprintHelperLibrary", "SpawnAIFromClass") else {
        return 0;
    };
    let Some(cdo) = cls.class_default_object() else {
        return 0;
    };
    let mut parms = Parms {
        world_context: world_ctx as u64,
        pawn_class: class_ptr,
        behavior_tree: 0,
        location: [x, y, z],
        rotation: [0.0; 3],
        b_no_collision_fail: 1,
        _pad: [0; 7],
        owner: 0,
        return_value: 0,
    };
    // SAFETY: cdo and func are live; parms matches the 0x60-byte
    // parm block measured from the object dump and proven by
    // research_spawn::spawn_one_npc.
    unsafe {
        cdo.process_event(func, &mut parms as *mut Parms as *mut c_void);
    }
    parms.return_value
}

fn register_ops() {
    ueforge::ops::OP_REGISTRY.register_many([
        ueforge::ops::OpDef::new(
            "spawning_stats",
            "Scaling spawner counters",
            "{}",
            |_args| {
                Ok(serde_json::json!({
                    "spawned_total": SPAWNED_TOTAL.load(Ordering::Relaxed),
                    "squares_processed": SQUARES_PROCESSED.load(Ordering::Relaxed),
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
