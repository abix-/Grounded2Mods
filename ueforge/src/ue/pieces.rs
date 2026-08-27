//! Reading a level's actors as pieces, and putting pieces back.
//!
//! A level is not one lump. It is a set of placed actors:
//! buildings, walls, rocks, fences, containers. Each is a piece
//! with a class, a position, a facing, a size and often a mesh.
//! Reading them and spawning them are both Unreal work that no
//! single game owns, so they live here.
//!
//! Pieces are [`modforge::structure::PieceDef`]. There is one
//! piece type in this workspace and this is it: a game mod
//! defining its own is how two of them end up disagreeing about
//! what a piece is.
//!
//! # The two spaces
//!
//! Unreal measures in centimetres with z up and angles in
//! degrees. modforge measures in metres with y up and angles in
//! radians. This module is the ONE place that converts between
//! them, in both directions.
//!
//! The axis map (mf `x,y,z` to ue `-z,x,y`) flips handedness: its
//! determinant is -1. Under a reflection, angles REVERSE, so a
//! facing is not merely rescaled but negated and offset. Getting
//! that wrong is silent and looks like two of a room's four walls
//! running backwards into the room. Two callers wrote this
//! conversion separately and disagreed about exactly that, which
//! is why it is here once.

use std::collections::HashMap;

use glam::Vec3;
use modforge::structure::PieceDef;

use super::transform;

/// Unreal centimetres in a modforge metre.
const CM_PER_M: f64 = 100.0;

/// Long enough for a busy frame during streaming; short enough
/// that a wedged game thread returns an error rather than hanging
/// the caller.
const ENGINE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// A piece read out of a level, still in Unreal's numbers.
///
/// Kept only long enough to work out the set's middle, because
/// offsets are relative to that. Converted to [`PieceDef`]
/// immediately after.
struct RawPiece {
    class: String,
    x: f64,
    y: f64,
    z: f64,
    pitch: f64,
    yaw: f64,
    roll: f64,
    scale: f64,
    mesh: Option<String>,
    extent: (f64, f64, f64),
}

/// Read every actor in levels whose path contains `path_needle`
/// as pieces, with offsets relative to the set's own middle and
/// its lowest point.
///
/// Relative offsets are what let a set be put down anywhere
/// afterwards. `only`, when not empty, keeps just the classes
/// whose names contain one of its entries.
///
/// Game thread only: it reads live objects.
pub fn read_level(path_needle: &str, only: &[String]) -> Vec<PieceDef> {
    let mut raw: Vec<RawPiece> = Vec::new();
    for (class, ptr) in super::actor::actors_in_levels(path_needle) {
        if !only.is_empty() && !only.iter().any(|c| class.contains(c.as_str())) {
            continue;
        }
        // SAFETY: ptr came from that call's own object iteration.
        let Some(t) = (unsafe { transform::read(ptr) }) else {
            continue;
        };
        // SAFETY: as above.
        let mesh = unsafe { transform::static_mesh(ptr) };
        let (mesh, extent) = match mesh {
            Some((name, ex, ey, ez)) => (Some(name), (ex, ey, ez)),
            None => (None, (0.0, 0.0, 0.0)),
        };
        raw.push(RawPiece {
            class,
            x: t.x,
            y: t.y,
            z: t.z,
            pitch: t.pitch,
            yaw: t.yaw,
            roll: t.roll,
            scale: t.scale_x,
            mesh,
            extent,
        });
    }
    if raw.is_empty() {
        return Vec::new();
    }

    let cx = raw.iter().map(|r| r.x).sum::<f64>() / raw.len() as f64;
    let cy = raw.iter().map(|r| r.y).sum::<f64>() / raw.len() as f64;
    let base_z = raw.iter().map(|r| r.z).fold(f64::MAX, f64::min);

    raw.into_iter()
        .map(|r| to_piece(&r, cx, cy, base_z))
        .collect()
}

/// One Unreal-space piece as a [`PieceDef`].
fn to_piece(r: &RawPiece, cx: f64, cy: f64, base_z: f64) -> PieceDef {
    PieceDef {
        class: r.class.clone(),
        asset: r.mesh.clone(),
        offset: Vec3::new(
            ((r.y - cy) / CM_PER_M) as f32,
            ((r.z - base_z) / CM_PER_M) as f32,
            (-(r.x - cx) / CM_PER_M) as f32,
        ),
        yaw: yaw_to_modforge(r.yaw),
        pitch: (r.pitch.to_radians()) as f32,
        roll: (r.roll.to_radians()) as f32,
        scale: r.scale as f32,
        // Extents are half-sizes, so only the axes swap round;
        // no sign changes.
        extent: Vec3::new(
            (r.extent.1 / CM_PER_M) as f32,
            (r.extent.2 / CM_PER_M) as f32,
            (r.extent.0 / CM_PER_M) as f32,
        ),
    }
}

/// Unreal facing (degrees) to modforge facing (radians).
///
/// Reversed, not merely converted: the axis map flips handedness.
pub fn yaw_to_modforge(ue_yaw_deg: f64) -> f32 {
    (90.0 - ue_yaw_deg).to_radians() as f32
}

/// modforge facing (radians) back to Unreal (degrees). The exact
/// inverse of [`yaw_to_modforge`].
pub fn yaw_to_unreal(mf_yaw_rad: f32) -> f64 {
    90.0 - (mf_yaw_rad as f64).to_degrees()
}

/// How a spawn went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Placed {
    pub placed: usize,
    pub failed: usize,
}

/// Spawn pieces into the world, centred on an Unreal-space point
/// and turned by `turn_deg` about the up axis.
///
/// `world_context` is any live actor in the target world.
///
/// Meshes are resolved by NAME through one index built up front,
/// not by pointer: a pointer is meaningless in a later session,
/// and searching per piece is a search per piece.
///
/// Game thread only.
///
/// # Safety
/// `world_context` must be a live actor.
pub unsafe fn spawn(
    world_context: *const u8,
    pieces: &[PieceDef],
    at: (f64, f64, f64),
    turn_deg: f64,
    limit: usize,
) -> Placed {
    let turn = turn_deg.to_radians();
    let (ts, tc) = turn.sin_cos();

    let need_meshes = pieces.iter().any(|p| p.asset.is_some());
    let meshes: HashMap<String, u64> = if need_meshes {
        transform::loaded_meshes()
    } else {
        HashMap::new()
    };

    let mut out = Placed::default();
    for piece in pieces.iter().take(limit) {
        let Some(class) = super::find_class_fast(&piece.class) else {
            out.failed += 1;
            continue;
        };
        // Back to Unreal's numbers to place it.
        let dx = -(piece.offset.z as f64) * CM_PER_M;
        let dy = piece.offset.x as f64 * CM_PER_M;
        let dz = piece.offset.y as f64 * CM_PER_M;
        let rx = dx * tc - dy * ts;
        let ry = dx * ts + dy * tc;
        let pos = (at.0 + rx, at.1 + ry, at.2 + dz);
        let yaw = yaw_to_unreal(piece.yaw).to_radians() + turn;
        let scale = (piece.scale as f64).max(0.01);

        // SAFETY: caller guarantees a live world context; the
        // class came from this frame's lookup; game thread.
        let actor = unsafe {
            super::spawn::begin_spawn(
                world_context,
                class.as_object().as_ptr() as u64,
                pos,
                yaw,
                scale,
            )
        };
        if actor == 0 {
            out.failed += 1;
            continue;
        }
        // A static mesh actor spawns empty: the copy needs the
        // same mesh, set before the spawn finishes.
        if let Some(name) = &piece.asset {
            if let Some(mesh) = meshes.get(name) {
                // SAFETY: actor came from begin_spawn above.
                unsafe { transform::set_actor_mesh(actor, *mesh) };
            }
        }
        // SAFETY: as above.
        if unsafe { super::spawn::finish_spawn(actor, pos, yaw, scale) } == 0 {
            out.failed += 1;
        } else {
            out.placed += 1;
        }
    }
    out
}

/// Every loaded static mesh whose name starts with `prefix`, with
/// its measured half-size and where its box sits relative to its
/// position marker, both in Unreal centimetres.
///
/// The marker offset is the part that matters for placement: a
/// wall marked at its base corner is laid differently from one
/// marked at its middle.
pub fn measured_meshes(prefix: &str) -> Vec<(String, (f64, f64, f64), (f64, f64, f64))> {
    let mut out = Vec::new();
    let Some(rt) = super::try_runtime() else {
        return out;
    };
    // SAFETY: rt came from try_runtime; image base and offsets
    // are what runtime init validated.
    let view = unsafe { super::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return out;
    }
    for obj in view.iter() {
        let is_mesh = obj
            .class()
            .map(|c| c.as_object().name() == "StaticMesh")
            .unwrap_or(false);
        if !is_mesh {
            continue;
        }
        let name = obj.name();
        if !name.starts_with(prefix) {
            continue;
        }
        // SAFETY: obj is a live UStaticMesh from this iteration.
        let (origin, extent) = unsafe { transform::mesh_bounds(obj.as_ptr()) };
        out.push((name, origin, extent));
    }
    out
}

/// One piece as JSON, in modforge's numbers.
fn piece_json(p: &PieceDef) -> serde_json::Value {
    serde_json::json!({
        "class": p.class,
        "asset": p.asset,
        "offset": [p.offset.x, p.offset.y, p.offset.z],
        "yaw": p.yaw,
        "pitch": p.pitch,
        "roll": p.roll,
        "scale": p.scale,
        "extent": [p.extent.x, p.extent.y, p.extent.z],
    })
}

/// A piece back from JSON. Missing fields take sane defaults so a
/// caller can send only what it cares about.
fn piece_from_json(v: &serde_json::Value) -> Option<PieceDef> {
    let f = |k: &str, d: f32| v.get(k).and_then(|x| x.as_f64()).unwrap_or(d as f64) as f32;
    let vec3 = |k: &str| {
        let a = v.get(k).and_then(|x| x.as_array());
        match a {
            Some(a) if a.len() == 3 => Vec3::new(
                a[0].as_f64().unwrap_or(0.0) as f32,
                a[1].as_f64().unwrap_or(0.0) as f32,
                a[2].as_f64().unwrap_or(0.0) as f32,
            ),
            _ => Vec3::ZERO,
        }
    };
    Some(PieceDef {
        class: v.get("class")?.as_str()?.to_string(),
        asset: v
            .get("asset")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        offset: vec3("offset"),
        yaw: f("yaw", 0.0),
        pitch: f("pitch", 0.0),
        roll: f("roll", 0.0),
        scale: f("scale", 1.0),
        extent: vec3("extent"),
    })
}

/// Register the piece endpoints with the workspace op registry.
///
/// Generic: no game names anywhere. A mod calls this once and
/// gets reading, measuring and placing over the control plane.
///
/// Every one of these enters the engine, so a consumer must route
/// them through its game-thread queue. They are registered here
/// as plain ops; wrapping them is the consumer's job.
pub fn register_ops() {
    crate::ops::OP_REGISTRY.register_many([
        crate::ops::OpDef::new(
            "level_pieces",
            "Read every actor in a level as pieces, offsets relative to the set's middle",
            "{level: str, classes?: [str]}",
            |args| {
                let level = crate::args::arg_str(args, "level")?.to_string();
                let classes: Vec<String> = args
                    .get("classes")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                // ON THE GAME THREAD. Reading the object list from
                // the control plane's own thread crashed the game
                // on 2026-08-27, faulting inside `read_level` on a
                // pointer the game had moved underneath it. The
                // ops in `ops.rs` were routed for this reason and
                // these were missed.
                crate::game_thread::run(
                    move || {
                        let pieces = read_level(&level, &classes);
                        Ok(serde_json::json!({
                            "level": level,
                            "count": pieces.len(),
                            "pieces": pieces.iter().map(piece_json).collect::<Vec<_>>(),
                        }))
                    },
                    ENGINE_TIMEOUT,
                )
            },
        ),
        crate::ops::OpDef::new(
            "level_classes",
            "What a level is made of: how many actors of each class",
            "{level: str}",
            |args| {
                let level = crate::args::arg_str(args, "level")?.to_string();
                let for_job = level.clone();
                // Game thread, as above.
                let counts = crate::game_thread::run(
                    move || Ok(serde_json::json!(class_counts(&for_job))),
                    ENGINE_TIMEOUT,
                )?;
                let counts: std::collections::HashMap<String, usize> =
                    serde_json::from_value(counts).map_err(|e| e.to_string())?;
                let total: usize = counts.values().sum();
                let mut rows: Vec<(String, usize)> = counts.into_iter().collect();
                rows.sort_by(|a, b| b.1.cmp(&a.1));
                Ok(serde_json::json!({
                    "level": level,
                    "actors": total,
                    "classes": rows
                        .into_iter()
                        .map(|(c, n)| serde_json::json!({"class": c, "count": n}))
                        .collect::<Vec<_>>(),
                }))
            },
        ),
        crate::ops::OpDef::new(
            "mesh_info",
            "Loaded static meshes by name prefix, with size and where the marker sits",
            "{prefix: str}",
            |args| {
                let prefix = crate::args::arg_str(args, "prefix")?.to_string();
                // Game thread, as above.
                let for_job = prefix.clone();
                let rows = crate::game_thread::run(
                    move || Ok(serde_json::json!(measured_meshes(&for_job))),
                    ENGINE_TIMEOUT,
                )?;
                let rows: Vec<(String, (f64, f64, f64), (f64, f64, f64))> =
                    serde_json::from_value(rows).map_err(|e| e.to_string())?;
                Ok(serde_json::json!({
                    "prefix": prefix,
                    "count": rows.len(),
                    "meshes": rows
                        .into_iter()
                        .map(|(name, o, e)| serde_json::json!({
                            "name": name,
                            // Doubled: callers think in sizes, the
                            // engine stores half-sizes.
                            "size": [e.0 * 2.0, e.1 * 2.0, e.2 * 2.0],
                            "marker_offset": [o.0, o.1, o.2],
                        }))
                        .collect::<Vec<_>>(),
                }))
            },
        ),
        crate::ops::OpDef::new(
            "place_pieces",
            "Spawn pieces into the world at a point, turned by a yaw",
            "{pieces: [piece], x: f64, y: f64, z: f64, turn?: f64, limit?: u64}",
            |args| {
                let raw = args
                    .get("pieces")
                    .and_then(|v| v.as_array())
                    .ok_or("need {pieces: [piece]}")?;
                let pieces: Vec<PieceDef> = raw.iter().filter_map(piece_from_json).collect();
                if pieces.is_empty() {
                    return Err("no usable pieces".into());
                }
                let at = (
                    crate::args::arg_f64(args, "x")?,
                    crate::args::arg_f64(args, "y")?,
                    crate::args::arg_f64(args, "z")?,
                );
                let turn = args.get("turn").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let limit = crate::args::arg_u64(args, "limit", Some(u64::MAX))? as usize;
                // Game thread: this searches for a world actor and
                // then spawns into it.
                crate::game_thread::run(
                    move || {
                        let ctx = super::actor::any_world_actor()
                            .ok_or("no level loaded, so nowhere to place anything")?;
                        // SAFETY: ctx is a live actor from the
                        // search just above, on the game thread.
                        let out = unsafe { spawn(ctx, &pieces, at, turn, limit) };
                        Ok(serde_json::json!({
                            "placed": out.placed,
                            "failed": out.failed,
                            "at": [at.0, at.1, at.2],
                        }))
                    },
                    ENGINE_TIMEOUT,
                )
            },
        ),
    ]);
}

/// The class histogram of a set of levels: what a place is made
/// of, before deciding what to read out of it.
pub fn class_counts(path_needle: &str) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (class, _) in super::actor::actors_in_levels(path_needle) {
        *counts.entry(class).or_default() += 1;
    }
    counts
}
