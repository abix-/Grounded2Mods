//! Captured structures and generated monuments.
//!
//! The game's places are levels full of placed actors. This
//! module lifts a coherent building and its surroundings out of a
//! live square as a `StructureDef` of captured pieces
//! (modforge::structure), keeps a library of them, and builds
//! MONUMENTS from that library using modforge's own arrangement
//! rules: the same machinery topside uses, over captured
//! structures instead of authored ones.
//!
//! misery-mod owns only the game-specific halves: reading actors
//! out of a UE level, and spawning them back. Everything generic
//! (what a structure is, how monuments arrange, footprints,
//! no-overlap) lives in modforge.
//!
//! The library feeds itself: every square donates structures, and
//! later squares are built from what earlier ones gave, so the
//! world composts into places nobody authored.

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use modforge::structure::{CONCRETE_FLOOR, CONCRETE_WALL, Library, PieceDef, StructureDef};

use crate::dispatch;


/// Pieces within this radius of a seed piece belong to the same
/// structure. Metres.
const STRUCTURE_RADIUS_M: f64 = 14.0;
/// A structure smaller than this is scenery litter, not a place.
const STRUCTURE_MIN_PIECES: usize = 6;
/// Bigger than this is most of a square; skip it.
const STRUCTURE_MAX_PIECES: usize = 70;
/// Structures taken from any one square.
const STRUCTURES_PER_SQUARE: usize = 3;
/// Library ceiling; oldest structures drop out past it.
const LIBRARY_CAP: usize = 60;
/// Chance a square receives a monument, once the library has
/// something worth building with.
const BUILD_CHANCE: f64 = 0.45;
/// Structures per generated monument.
const MEMBERS_PER_MONUMENT: (usize, usize) = (2, 5);
/// Keep monuments away from the square's edge. Centimetres.
const EDGE_MARGIN_CM: f64 = 1500.0;
/// Session ceiling on spawned pieces.
const SESSION_PIECE_CAP: u64 = 1500;
const POLL: Duration = Duration::from_secs(5);

/// What has been captured so far. The bounded store and the
/// random draw are modforge's (`structure::Library`); this only
/// holds one.
static LIBRARY: Mutex<Option<Library>> = Mutex::new(None);
static PIECES_SPAWNED: AtomicU64 = AtomicU64::new(0);
static MONUMENTS_BUILT: AtomicU64 = AtomicU64::new(0);
static STRUCTURES_CAPTURED: AtomicU64 = AtomicU64::new(0);

/// Cut a square's harvested pieces into structures: greedy
/// spatial grouping, so a building keeps the fence and the junk
/// pile that stand with it.
fn structures_from(source: &str, pieces: &[PieceDef]) -> Vec<StructureDef> {
    // Grouping loose pieces into things that stand together is
    // modforge's job (`group_nearby`), where it is unit-tested.
    // What stays here is only naming and colouring the result.
    modforge::structure::group_nearby(
        pieces,
        STRUCTURE_RADIUS_M as f32,
        STRUCTURE_MIN_PIECES,
        STRUCTURE_MAX_PIECES,
    )
    .into_iter()
    .map(|group| StructureDef {
        name: source.to_string(),
        wall_color: CONCRETE_WALL,
        floor_color: CONCRETE_FLOOR,
        rooms: Vec::new(),
        stairs: Vec::new(),
        furniture: Vec::new(),
        lights: Vec::new(),
        pieces: group,
    })
    .collect()
}

/// Take up to STRUCTURES_PER_SQUARE structures from a square into
/// the library, preferring the meatier ones.
fn donate(square: &str) {
    let pieces = ueforge::ue::pieces::read_level(square, &[]);
    if pieces.is_empty() {
        return;
    }
    let mut found = structures_from(square, &pieces);
    if found.is_empty() {
        return;
    }
    found.sort_by_key(|s| std::cmp::Reverse(s.pieces.len()));
    found.truncate(STRUCTURES_PER_SQUARE);
    let taken = found.len();
    let mut guard = LIBRARY.lock().unwrap_or_else(|e| e.into_inner());
    let lib = guard.get_or_insert_with(|| Library::new(LIBRARY_CAP));
    for s in found {
        lib.add(s);
    }
    let total = lib.len();
    drop(guard);
    STRUCTURES_CAPTURED.fetch_add(taken as u64, Ordering::Relaxed);
    ueforge::log::log(format_args!(
        "places: {square} donated {taken} structure(s), library now {total}"
    ));
}

/// Draw the members of one monument. Repeats are allowed: the
/// same building twice reads as a settlement that grew.
fn draw_members() -> Vec<StructureDef> {
    let guard = LIBRARY.lock().unwrap_or_else(|e| e.into_inner());
    let Some(lib) = guard.as_ref() else {
        return Vec::new();
    };
    let n = MEMBERS_PER_MONUMENT.0
        + fastrand::usize(0..=(MEMBERS_PER_MONUMENT.1 - MEMBERS_PER_MONUMENT.0));
    lib.draw(n)
}

pub fn install() {
    register_ops();
    // A stoppable worker, not a raw thread: shutdown wakes and
    // joins it so the DLL can unload without the loop still
    // executing freed code.
    std::mem::forget(modforge::rpg::poller::spawn_interval(
        "misery-places",
        POLL,
        // Reads live actors, so it runs on the game thread.
        ueforge::game_thread::each_tick(watcher),
    ));
}

/// Squares already given a chance at a monument. Lives outside
/// the tick because the worker calls back per interval.
static SEEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn watcher() {
    let mut guard = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    let seen = guard.get_or_insert_with(HashSet::new);
    {
        if ueforge::ue::try_runtime().is_none() {
            return;
        }
        let squares = crate::strange::live_squares();
        let live: HashSet<String> = squares.iter().map(|(n, _, _)| n.clone()).collect();
        seen.retain(|s| live.contains(s));

        let Some(tile) = crate::strange::active_tile_size() else { return };
        for (name, cx, cy) in squares {
            if seen.contains(&name) {
                continue;
            }
            seen.insert(name.clone());

            // Every square first gives, then may receive.
            donate(&name);

            if PIECES_SPAWNED.load(Ordering::Relaxed) >= SESSION_PIECE_CAP {
                continue;
            }
            if fastrand::f64() >= BUILD_CHANCE {
                continue;
            }
            let members = draw_members();
            if members.is_empty() {
                continue;
            }
            let half = (tile / 2.0 - EDGE_MARGIN_CM).max(0.0);
            let centre = (
                cx as f64 * tile + (fastrand::f64() * 2.0 - 1.0) * half,
                cy as f64 * tile + (fastrand::f64() * 2.0 - 1.0) * half,
            );
            let sources: Vec<&str> = members.iter().map(|s| s.name.as_str()).collect();
            ueforge::log::log(format_args!(
                "places: monument in {name} from {} structure(s) {sources:?}",
                members.len()
            ));
            // `run`, not `enqueue`: this watcher already runs ON
            // the game thread, and queueing from there waits for
            // a drain that cannot start until we return.
            let r = ueforge::game_thread::run(
                move || build_monument(members, centre),
                Duration::from_secs(30),
            );
            if let Err(e) = r {
                ueforge::log::log(format_args!("places: monument failed: {e}"));
            }
        }
    }
}

/// Game thread. Put one monument in the world.
///
/// Finding the ground, laying the buildings out and spawning the
/// pieces is `ueforge::ue::pieces::place_monument`. What is
/// MISERY's here is the session budget, this game's trace range,
/// and the counters.
fn build_monument(
    members: Vec<StructureDef>,
    centre: (f64, f64),
) -> Result<serde_json::Value, String> {
    let remaining = SESSION_PIECE_CAP.saturating_sub(PIECES_SPAWNED.load(Ordering::Relaxed));
    if remaining == 0 {
        return Err("session piece cap reached".into());
    }
    let placer = ueforge::ue::pieces::UePlacer {
        up: crate::TRACE_UP,
        down: crate::TRACE_DOWN,
    };
    let out = modforge::monument::place_monument(
        &placer,
        members,
        centre.0,
        centre.1,
        fastrand::f64() * 360.0,
        remaining as usize,
    )?;
    PIECES_SPAWNED.fetch_add(out.placed as u64, Ordering::Relaxed);
    MONUMENTS_BUILT.fetch_add(1, Ordering::Relaxed);
    ueforge::log::log(format_args!(
        "places: monument placed {} piece(s), {:?}",
        out.placed, out.arrangement
    ));
    Ok(serde_json::json!({
        "placed": out.placed,
        "failed": out.failed,
        "arrangement": format!("{:?}", out.arrangement),
    }))
}

fn register_ops() {
    ueforge::ops::OP_REGISTRY.register_many([
        ueforge::ops::OpDef::new("places_stats", "Captured structures and monuments", "{}", |_a| {
            let guard = LIBRARY.lock().unwrap_or_else(|e| e.into_inner());
            let held: &[StructureDef] = guard.as_ref().map(|l| l.items()).unwrap_or(&[]);
            Ok(serde_json::json!({
                "library_structures": held.len(),
                "structure_sizes": held.iter().map(|s| s.pieces.len()).collect::<Vec<_>>(),
                "sources": held.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
                "structures_captured": STRUCTURES_CAPTURED.load(Ordering::Relaxed),
                "monuments_built": MONUMENTS_BUILT.load(Ordering::Relaxed),
                "pieces_spawned": PIECES_SPAWNED.load(Ordering::Relaxed),
            }))
        }),
        ueforge::ops::OpDef::new(
            "build_monument_here",
            "Build one generated monument at the player (testing)",
            "{}",
            |_a| {
                let members = draw_members();
                if members.is_empty() {
                    return Err("library is empty; walk a square first".into());
                }
                dispatch::DRAIN.queue().enqueue(
                    move || {
                        let player = ueforge::ue::actor::find_actors_by_chain(
                            "BP_SGKMasterCharacter_C",
                        )
                        .into_iter()
                        .next()
                        .ok_or("no player")?;
                        // SAFETY: live player actor, game thread.
                        let here = unsafe {
                            ueforge::ue::transform::world_location(player)
                        }
                        .ok_or("no location")?;
                        build_monument(members, (here.0, here.1))
                    },
                    Duration::from_secs(30),
                )
            },
        ),
    ]);
}
