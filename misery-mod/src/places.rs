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

use glam::Vec3;
use modforge::monument::{Arrangement, Roll, arrange};
use modforge::structure::{CONCRETE_FLOOR, CONCRETE_WALL, PieceDef, StructureDef};

use crate::dispatch;
use crate::harvest::{self, Composition, Piece};

/// UE works in centimetres and z-up; modforge in metres and y-up
/// (north = -z). Conversions live here, at the binder edge.
const CM_PER_M: f64 = 100.0;

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

static LIBRARY: Mutex<Vec<StructureDef>> = Mutex::new(Vec::new());
static PIECES_SPAWNED: AtomicU64 = AtomicU64::new(0);
static MONUMENTS_BUILT: AtomicU64 = AtomicU64::new(0);
static STRUCTURES_CAPTURED: AtomicU64 = AtomicU64::new(0);

/// Cut a square's harvested pieces into structures: greedy
/// spatial grouping, so a building keeps the fence and the junk
/// pile that stand with it.
fn structures_from(comp: &Composition) -> Vec<StructureDef> {
    let radius_cm = STRUCTURE_RADIUS_M * CM_PER_M;
    let mut taken = vec![false; comp.pieces.len()];
    let mut out = Vec::new();
    for i in 0..comp.pieces.len() {
        if taken[i] {
            continue;
        }
        let seed = &comp.pieces[i];
        let mut members: Vec<usize> = Vec::new();
        for (j, other) in comp.pieces.iter().enumerate() {
            if taken[j] {
                continue;
            }
            let dx = other.dx - seed.dx;
            let dy = other.dy - seed.dy;
            if dx * dx + dy * dy <= radius_cm * radius_cm {
                members.push(j);
            }
            if members.len() > STRUCTURE_MAX_PIECES {
                break;
            }
        }
        if members.len() < STRUCTURE_MIN_PIECES || members.len() > STRUCTURE_MAX_PIECES {
            taken[i] = true;
            continue;
        }
        // Re-centre on the group's own middle so it can be placed
        // anywhere and turned about a sane pivot.
        let cx = members.iter().map(|&j| comp.pieces[j].dx).sum::<f64>() / members.len() as f64;
        let cy = members.iter().map(|&j| comp.pieces[j].dy).sum::<f64>() / members.len() as f64;
        let base_z = members
            .iter()
            .map(|&j| comp.pieces[j].dz)
            .fold(f64::MAX, f64::min);
        let pieces: Vec<PieceDef> = members
            .iter()
            .map(|&j| to_piece_spec(&comp.pieces[j], cx, cy, base_z))
            .collect();
        for &j in &members {
            taken[j] = true;
        }
        out.push(StructureDef {
            name: comp.source.clone(),
            wall_color: CONCRETE_WALL,
            floor_color: CONCRETE_FLOOR,
            rooms: Vec::new(),
            stairs: Vec::new(),
            furniture: Vec::new(),
            lights: Vec::new(),
            pieces,
        });
    }
    out
}

/// UE piece (cm, z-up) to modforge PieceDef (m, y-up, north -z).
/// Extents are half-sizes, so only the axes swap, not the signs.
fn to_piece_spec(p: &Piece, cx: f64, cy: f64, base_z: f64) -> PieceDef {
    PieceDef {
        class: p.class.clone(),
        asset: p.mesh.clone(),
        offset: Vec3::new(
            ((p.dy - cy) / CM_PER_M) as f32,
            ((p.dz - base_z) / CM_PER_M) as f32,
            (-(p.dx - cx) / CM_PER_M) as f32,
        ),
        yaw: p.yaw.to_radians() as f32,
        pitch: p.pitch.to_radians() as f32,
        roll: p.roll.to_radians() as f32,
        scale: p.scale as f32,
        extent: Vec3::new(
            (p.ey / CM_PER_M) as f32,
            (p.ez / CM_PER_M) as f32,
            (p.ex / CM_PER_M) as f32,
        ),
    }
}

/// modforge PieceDef back to a UE-space Piece for spawning.
fn to_piece(spec: &PieceDef, member_offset: Vec3) -> Piece {
    let o = spec.offset + member_offset;
    Piece {
        class: spec.class.clone(),
        dx: -(o.z as f64) * CM_PER_M,
        dy: o.x as f64 * CM_PER_M,
        dz: o.y as f64 * CM_PER_M,
        yaw: (spec.yaw as f64).to_degrees(),
        pitch: (spec.pitch as f64).to_degrees(),
        roll: (spec.roll as f64).to_degrees(),
        scale: spec.scale as f64,
        mesh: spec.asset.clone(),
        ex: (spec.extent.z as f64) * CM_PER_M,
        ey: (spec.extent.x as f64) * CM_PER_M,
        ez: (spec.extent.y as f64) * CM_PER_M,
    }
}

/// Take up to STRUCTURES_PER_SQUARE structures from a square into
/// the library, preferring the meatier ones.
fn donate(square: &str) {
    let Ok(comp) = harvest::harvest_level(square) else { return };
    let mut found = structures_from(&comp);
    if found.is_empty() {
        return;
    }
    found.sort_by_key(|s| std::cmp::Reverse(s.pieces.len()));
    found.truncate(STRUCTURES_PER_SQUARE);
    let taken = found.len();
    let mut lib = LIBRARY.lock().unwrap_or_else(|e| e.into_inner());
    lib.extend(found);
    let overflow = lib.len().saturating_sub(LIBRARY_CAP);
    if overflow > 0 {
        lib.drain(0..overflow);
    }
    let total = lib.len();
    drop(lib);
    STRUCTURES_CAPTURED.fetch_add(taken as u64, Ordering::Relaxed);
    ueforge::log::log(format_args!(
        "places: {square} donated {taken} structure(s), library now {total}"
    ));
}

/// Draw the members of one monument. Repeats are allowed: the
/// same building twice reads as a settlement that grew.
fn draw_members() -> Vec<StructureDef> {
    let lib = LIBRARY.lock().unwrap_or_else(|e| e.into_inner());
    if lib.is_empty() {
        return Vec::new();
    }
    let n = MEMBERS_PER_MONUMENT.0
        + fastrand::usize(0..=(MEMBERS_PER_MONUMENT.1 - MEMBERS_PER_MONUMENT.0));
    (0..n).map(|_| lib[fastrand::usize(0..lib.len())].clone()).collect()
}

pub fn install() {
    register_ops();
    // A stoppable worker, not a raw thread: shutdown wakes and
    // joins it so the DLL can unload without the loop still
    // executing freed code.
    std::mem::forget(modforge::rpg::poller::spawn_interval(
        "misery-places",
        POLL,
        watcher,
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
            let r = dispatch::DRAIN.queue().enqueue(
                move || build_monument(members, centre),
                Duration::from_secs(30),
            );
            if let Err(e) = r {
                ueforge::log::log(format_args!("places: monument failed: {e}"));
            }
        }
    }
}

/// Game thread. Arrange the structures with modforge's rules,
/// then spawn each member's pieces at its arranged offset.
fn build_monument(
    members: Vec<StructureDef>,
    centre: (f64, f64),
) -> Result<serde_json::Value, String> {
    let Some(z) =
        ueforge::ue::trace::ground_at(centre.0, centre.1, crate::TRACE_UP, crate::TRACE_DOWN)
    else {
        return Err("no ground under the monument".into());
    };
    // A seed from the spot itself: the same place rolls the same
    // layout, different places differ.
    let seed = (centre.0.abs() as u64) << 20 ^ (centre.1.abs() as u64);
    let mut roll = Roll::new(seed);
    let arrangement = *roll.pick(&[
        Arrangement::Clustered,
        Arrangement::AroundYard,
        Arrangement::AlongRoad,
    ]);
    let turn = fastrand::f64() * 360.0;
    let placed_members = arrange(members, arrangement, &mut roll);

    let mut placed = 0usize;
    for member in &placed_members {
        if PIECES_SPAWNED.load(Ordering::Relaxed) >= SESSION_PIECE_CAP {
            break;
        }
        let comp = Composition {
            source: member.structure.name.clone(),
            pieces: member
                .structure
                .pieces
                .iter()
                .map(|p| to_piece(p, member.offset))
                .collect(),
        };
        let n = harvest::place_composition_at(&comp, centre.0, centre.1, z, turn, usize::MAX)?;
        PIECES_SPAWNED.fetch_add(n as u64, Ordering::Relaxed);
        placed += n;
    }
    MONUMENTS_BUILT.fetch_add(1, Ordering::Relaxed);
    ueforge::log::log(format_args!(
        "places: monument placed {placed} piece(s), {arrangement:?}"
    ));
    Ok(serde_json::json!({"placed": placed, "arrangement": format!("{arrangement:?}")}))
}

fn register_ops() {
    ueforge::ops::OP_REGISTRY.register_many([
        ueforge::ops::OpDef::new("places_stats", "Captured structures and monuments", "{}", |_a| {
            let lib = LIBRARY.lock().unwrap_or_else(|e| e.into_inner());
            Ok(serde_json::json!({
                "library_structures": lib.len(),
                "structure_sizes": lib.iter().map(|s| s.pieces.len()).collect::<Vec<_>>(),
                "sources": lib.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
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
