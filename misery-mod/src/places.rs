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

use std::sync::Mutex;
use std::time::Duration;

use modforge::monument::{PiecePlacer, PlaceSource, TakeAndBuild};

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

/// MISERY's answers to what `TakeAndBuild` asks: which squares
/// are loaded, how big a square is, what is in one, how high the
/// ground is, and put these pieces in the world.
///
/// The last two are Unreal's and come from `UePlacer`; the first
/// three are this game's. Everything decided FROM the answers
/// lives in modforge.
struct MiserySquares(ueforge::ue::pieces::UePlacer);

impl PiecePlacer for MiserySquares {
    fn ground_at(&self, x: f64, y: f64) -> Option<f64> {
        self.0.ground_at(x, y)
    }
    fn spawn(
        &self,
        pieces: &[modforge::structure::PieceDef],
        at: (f64, f64, f64),
        turn_deg: f64,
        limit: usize,
    ) -> modforge::monument::Placed {
        self.0.spawn(pieces, at, turn_deg, limit)
    }
}

impl PlaceSource for MiserySquares {
    fn live_places(&self) -> Vec<(String, i32, i32)> {
        crate::strange::live_squares()
    }
    fn cell_size(&self) -> Option<f64> {
        crate::strange::active_tile_size()
    }
    fn read_pieces(&self, place: &str) -> Vec<modforge::structure::PieceDef> {
        ueforge::ue::pieces::read_level(place, &[])
    }
}

fn squares() -> MiserySquares {
    MiserySquares(ueforge::ue::pieces::UePlacer {
        up: crate::TRACE_UP,
        down: crate::TRACE_DOWN,
    })
}

/// The one instance. Its numbers are MISERY's; everything it
/// does with them is modforge's.
static TAKER: Mutex<Option<TakeAndBuild>> = Mutex::new(None);

fn with_taker<T>(f: impl FnOnce(&mut TakeAndBuild) -> T) -> T {
    let mut guard = TAKER.lock().unwrap_or_else(|e| e.into_inner());
    let taker = guard.get_or_insert_with(|| {
        let mut t = TakeAndBuild::new(LIBRARY_CAP);
        t.radius = STRUCTURE_RADIUS_M as f32;
        t.piece_range = (STRUCTURE_MIN_PIECES, STRUCTURE_MAX_PIECES);
        t.per_place = STRUCTURES_PER_SQUARE;
        t.build_chance = BUILD_CHANCE;
        t.members = MEMBERS_PER_MONUMENT;
        t.margin = EDGE_MARGIN_CM;
        t.piece_budget = SESSION_PIECE_CAP;
        t
    });
    f(taker)
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

fn watcher() {
    if ueforge::ue::try_runtime().is_none() {
        return;
    }
    let source = squares();
    let report = with_taker(|t| t.tick(&source));
    if report.monuments_built > 0 || report.structures_taken > 0 {
        ueforge::log::log(format_args!(
            "places: {} place(s), took {} structure(s), built {} monument(s), {} piece(s)",
            report.places_considered,
            report.structures_taken,
            report.monuments_built,
            report.pieces_placed,
        ));
    }
}

fn register_ops() {
    ueforge::ops::OP_REGISTRY.register_many([
        ueforge::ops::OpDef::new("places_stats", "Captured structures and monuments", "{}", |_a| {
            with_taker(|t| {
                let held = t.library().items();
                Ok(serde_json::json!({
                    "library_structures": held.len(),
                    "structure_sizes": held.iter().map(|s| s.pieces.len()).collect::<Vec<_>>(),
                    "sources": held.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
                    "structures_captured": t.structures_captured(),
                    "monuments_built": t.monuments_built(),
                    "pieces_spawned": t.pieces_placed(),
                }))
            })
        }),
        ueforge::ops::OpDef::new(
            "build_monument_here",
            "Build one generated monument at the player (testing)",
            "{}",
            |_a| {
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
                        let source = squares();
                        let out = with_taker(|t| {
                            t.build_one(&source, here.0, here.1, fastrand::f64() * 360.0)
                        })?;
                        Ok(serde_json::json!({
                            "placed": out.placed,
                            "failed": out.failed,
                            "arrangement": format!("{:?}", out.arrangement),
                        }))
                    },
                    Duration::from_secs(30),
                )
            },
        ),
    ]);
}
