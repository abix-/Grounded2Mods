//! Harvest and compose: build new places out of the pieces the
//! game's own map squares are made of.
//!
//! A square is not atomic. It is a level full of placed actors:
//! buildings, walls, rocks, fences, containers. This module reads
//! those pieces out of a live square with their transforms
//! (relative to the square's centre, so a harvest is reusable
//! anywhere), and spawns a saved arrangement back into the world
//! at a new location.
//!
//! Transform reading is straight memory, not UFunction calls:
//! AActor::RootComponent +0x1A0, then USceneComponent
//! RelativeLocation +0x128, RelativeRotation +0x140,
//! RelativeScale3D +0x158.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use ueforge::ue::{self, UObject, read_at};

use crate::dispatch;

const ROOT_COMPONENT_OFFSET: usize = 0x1A0;
const RELATIVE_LOCATION_OFFSET: usize = 0x128;
const RELATIVE_ROTATION_OFFSET: usize = 0x140;
const RELATIVE_SCALE_OFFSET: usize = 0x158;
/// StaticMeshActor::StaticMeshComponent, and the mesh asset on it.
const STATIC_MESH_COMPONENT_OFFSET: usize = 0x290;
const STATIC_MESH_OFFSET: usize = 0x560;
/// UStaticMesh::ExtendedBounds (FBoxSphereBounds), and BoxExtent
/// within it: the half-size of the mesh's own geometry.
const EXTENDED_BOUNDS_OFFSET: usize = 0x1F0;
const BOX_EXTENT_OFFSET: usize = 0x18;
/// USceneComponent::Mobility. 0 Static, 1 Stationary, 2 Movable.
const MOBILITY_MOVABLE: u8 = 2;

/// One harvested piece: which class, and where it sits relative
/// to the square's centre.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Piece {
    pub class: String,
    /// Offset from the square centre, in world units.
    pub dx: f64,
    pub dy: f64,
    pub dz: f64,
    /// Degrees, UE convention.
    pub yaw: f64,
    #[serde(default)]
    pub pitch: f64,
    #[serde(default)]
    pub roll: f64,
    pub scale: f64,
    /// Half-size of the piece's own geometry, UE centimetres, from
    /// the mesh asset's bounds. Zero when unmeasurable. This is
    /// what lets a piece be classified as wall, floor, or post.
    #[serde(default)]
    pub ex: f64,
    #[serde(default)]
    pub ey: f64,
    #[serde(default)]
    pub ez: f64,
    /// For StaticMeshActor pieces: the mesh asset's name. The
    /// class alone spawns an empty actor, so the mesh has to be
    /// re-resolved and set on the copy. Names (not pointers) so a
    /// saved composition survives a restart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh: Option<String>,
}

/// A saved arrangement: the pieces of one place.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Composition {
    pub source: String,
    pub pieces: Vec<Piece>,
}

/// World-space transform of an actor, via its root component:
/// position, full rotation (pitch, yaw, roll), and scale.
/// FRotator is stored pitch, yaw, roll.
fn actor_transform(actor: *const u8) -> Option<(f64, f64, f64, f64, f64, f64, f64)> {
    // SAFETY: actor is a live UObject from a GObjects walk; the
    // RootComponent slot is a UE-stable AActor field.
    let root: *const u8 = unsafe { read_at(actor, ROOT_COMPONENT_OFFSET) };
    if root.is_null() {
        return None;
    }
    // SAFETY: root is a live USceneComponent; the three transform
    // fields are at documented offsets.
    unsafe {
        let x: f64 = read_at(root, RELATIVE_LOCATION_OFFSET);
        let y: f64 = read_at(root, RELATIVE_LOCATION_OFFSET + 8);
        let z: f64 = read_at(root, RELATIVE_LOCATION_OFFSET + 16);
        let pitch: f64 = read_at(root, RELATIVE_ROTATION_OFFSET);
        let yaw: f64 = read_at(root, RELATIVE_ROTATION_OFFSET + 8);
        let roll: f64 = read_at(root, RELATIVE_ROTATION_OFFSET + 16);
        let sx: f64 = read_at(root, RELATIVE_SCALE_OFFSET);
        Some((x, y, z, pitch, yaw, roll, sx))
    }
}

/// The mesh asset name and its local half-extent (UE cm) on a
/// StaticMeshActor, if it has one.
fn actor_mesh(actor: *const u8) -> Option<(String, f64, f64, f64)> {
    // SAFETY: actor is live; the offsets are documented engine
    // fields (StaticMeshActor -> its component -> the asset), and
    // ExtendedBounds is FBoxSphereBounds { Origin, BoxExtent,
    // SphereRadius } with BoxExtent at +0x18.
    unsafe {
        let comp: *const u8 = read_at(actor, STATIC_MESH_COMPONENT_OFFSET);
        if comp.is_null() {
            return None;
        }
        let mesh: *const u8 = read_at(comp, STATIC_MESH_OFFSET);
        let name = (mesh as *const UObject).as_ref()?.name();
        let at = EXTENDED_BOUNDS_OFFSET + BOX_EXTENT_OFFSET;
        Some((
            name,
            read_at::<f64>(mesh, at),
            read_at::<f64>(mesh, at + 8),
            read_at::<f64>(mesh, at + 16),
        ))
    }
}

/// Every actor owned by a level whose path contains `needle`,
/// as (class name, actor pointer).
fn level_actors(needle: &str) -> Vec<(String, *const u8)> {
    let mut out = Vec::new();
    let Some(rt) = ue::try_runtime() else { return out };
    // SAFETY: runtime came from try_runtime; the view is built
    // from the validated image base + offsets.
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return out;
    }
    for obj in view.iter() {
        if obj.is_default_object() {
            continue;
        }
        let full = obj.full_name();
        if !full.contains(needle) || !full.contains(".PersistentLevel.") {
            continue;
        }
        // Components live under the actor and carry a further dot
        // in the tail; keep only the actor itself.
        let Some(tail) = full.split(".PersistentLevel.").nth(1) else { continue };
        if tail.contains('.') {
            continue;
        }
        let class = obj.class().map(|c| c.as_object().name()).unwrap_or_default();
        out.push((class, obj.as_ptr()));
    }
    out
}

/// Class histogram of a square: what a place is actually made of.
fn harvest_classes(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let needle = args
        .get("level")
        .and_then(|v| v.as_str())
        .ok_or("need {level: str}")?;
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (class, _) in level_actors(needle) {
        *counts.entry(class).or_default() += 1;
    }
    let mut rows: Vec<(String, usize)> = counts.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    let total: usize = rows.iter().map(|r| r.1).sum();
    Ok(serde_json::json!({
        "level": needle,
        "actors": total,
        "classes": rows.iter().map(|(c, n)| serde_json::json!({"class": c, "count": n}))
            .collect::<Vec<_>>(),
    }))
}

/// Harvest a square into a Composition: every piece with its
/// offset from the square centre. `centre_x` / `centre_y` come
/// from the caller (worldgen.md 4.2: cell * TileSize); when
/// omitted the harvest is centred on the pieces' own midpoint.
fn harvest_square(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let needle = args
        .get("level")
        .and_then(|v| v.as_str())
        .ok_or("need {level: str}")?;
    let only: Vec<String> = args
        .get("classes")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let centre = match (
        args.get("centre_x").and_then(|v| v.as_f64()),
        args.get("centre_y").and_then(|v| v.as_f64()),
    ) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    };
    let comp = harvest_with(needle, &only, centre)?;
    serde_json::to_value(&comp).map_err(|e| e.to_string())
}

/// Harvest every piece of a level, centred on the pieces' own
/// midpoint. The library path (`places.rs`) uses this.
pub fn harvest_level(needle: &str) -> Result<Composition, String> {
    harvest_with(needle, &[], None)
}

fn harvest_with(
    needle: &str,
    only: &[String],
    centre: Option<(f64, f64)>,
) -> Result<Composition, String> {
    struct Raw {
        class: String,
        x: f64,
        y: f64,
        z: f64,
        pitch: f64,
        yaw: f64,
        roll: f64,
        scale: f64,
        mesh: Option<(String, f64, f64, f64)>,
    }
    let mut raw: Vec<Raw> = Vec::new();
    for (class, ptr) in level_actors(needle) {
        if !only.is_empty() && !only.iter().any(|c| class.contains(c.as_str())) {
            continue;
        }
        if let Some((x, y, z, pitch, yaw, roll, scale)) = actor_transform(ptr) {
            let mesh = if class == "StaticMeshActor" {
                actor_mesh(ptr)
            } else {
                None
            };
            raw.push(Raw { class, x, y, z, pitch, yaw, roll, scale, mesh });
        }
    }
    if raw.is_empty() {
        return Err(format!("no actors harvested from '{needle}'"));
    }

    let (cx, cy) = centre.unwrap_or((
        raw.iter().map(|r| r.x).sum::<f64>() / raw.len() as f64,
        raw.iter().map(|r| r.y).sum::<f64>() / raw.len() as f64,
    ));
    let base_z = raw.iter().map(|r| r.z).fold(f64::MAX, f64::min);

    Ok(Composition {
        source: needle.to_string(),
        pieces: raw
            .into_iter()
            .map(|r| {
                let (mesh, ex, ey, ez) = match r.mesh {
                    Some((name, ex, ey, ez)) => (Some(name), ex, ey, ez),
                    None => (None, 0.0, 0.0, 0.0),
                };
                Piece {
                    class: r.class,
                    dx: r.x - cx,
                    dy: r.y - cy,
                    dz: r.z - base_z,
                    yaw: r.yaw,
                    pitch: r.pitch,
                    roll: r.roll,
                    scale: r.scale,
                    mesh,
                    ex,
                    ey,
                    ez,
                }
            })
            .collect(),
    })
}

/// Spawn a saved Composition at a world position, optionally
/// rotated. Runs on the game thread.
fn compose(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let comp: Composition = serde_json::from_value(
        args.get("composition").cloned().ok_or("need {composition: ...}")?,
    )
    .map_err(|e| format!("bad composition: {e}"))?;
    let at_x = args.get("x").and_then(|v| v.as_f64());
    let at_y = args.get("y").and_then(|v| v.as_f64());
    let at_z = args.get("z").and_then(|v| v.as_f64());
    let turn = args.get("yaw").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let limit = args
        .get("max")
        .and_then(|v| v.as_u64())
        .unwrap_or(400) as usize;

    dispatch::DRAIN.queue().enqueue(
        move || place_composition(&comp, at_x, at_y, at_z, turn, limit),
        Duration::from_secs(30),
    )
}

/// Game thread. Spawn each piece at centre + rotated offset.
fn place_composition(
    comp: &Composition,
    at_x: Option<f64>,
    at_y: Option<f64>,
    at_z: Option<f64>,
    turn_deg: f64,
    limit: usize,
) -> Result<serde_json::Value, String> {
    let player = ueforge::ue::actor::find_actors_by_chain("BP_SGKMasterCharacter_C")
        .into_iter()
        .next()
        .ok_or("no player")?;
    let here = crate::strange::actor_location(player).ok_or("no player location")?;
    let placed = place_composition_at(
        comp,
        at_x.unwrap_or(here.0),
        at_y.unwrap_or(here.1),
        at_z.unwrap_or(here.2),
        turn_deg,
        limit,
    )?;
    Ok(serde_json::json!({"placed": placed}))
}

/// Game thread. Spawn a composition centred on a world point,
/// turned by `turn_deg`. Returns how many pieces landed.
pub fn place_composition_at(
    comp: &Composition,
    cx: f64,
    cy: f64,
    cz: f64,
    turn_deg: f64,
    limit: usize,
) -> Result<usize, String> {
    let player = ueforge::ue::actor::find_actors_by_chain("BP_SGKMasterCharacter_C")
        .into_iter()
        .next()
        .ok_or("no player")?;
    let turn = turn_deg.to_radians();
    let (ts, tc) = turn.sin_cos();

    // Meshes are re-resolved by name so a saved composition works
    // in a later session; one map beats a GObjects walk per piece.
    let need_meshes = comp.pieces.iter().any(|p| p.mesh.is_some());
    let meshes = if need_meshes { mesh_index() } else { HashMap::new() };

    let mut placed = 0usize;
    let mut failed = 0usize;
    for piece in comp.pieces.iter().take(limit) {
        let Some(class) = ue::find_class_fast(&piece.class) else {
            failed += 1;
            continue;
        };
        let rx = piece.dx * tc - piece.dy * ts;
        let ry = piece.dx * ts + piece.dy * tc;
        let yaw = piece.yaw.to_radians() + turn;
        let (px, py, pz) = (cx + rx, cy + ry, cz + piece.dz);

        let actor = crate::strange::begin_spawn(
            player,
            class.as_object().as_ptr() as u64,
            px,
            py,
            pz,
            yaw,
            piece.scale.max(0.01),
        );
        if actor == 0 {
            failed += 1;
            continue;
        }
        // A StaticMeshActor spawns empty: give the copy the same
        // mesh, and make it Movable first (a Static component
        // refuses changes after registration).
        if let Some(name) = &piece.mesh {
            if let Some(mesh_ptr) = meshes.get(name) {
                apply_mesh(actor, *mesh_ptr);
            }
        }
        if crate::strange::finish_spawn(actor, px, py, pz, yaw, piece.scale.max(0.01)) == 0 {
            failed += 1;
        } else {
            placed += 1;
        }
    }
    ueforge::log::log(format_args!(
        "harvest: composed {placed} piece(s) from {} ({failed} failed)",
        comp.source
    ));
    Ok(placed)
}

/// Name to pointer for every loaded UStaticMesh. Built once per
/// compose so pieces resolve without a walk each.
fn mesh_index() -> HashMap<String, u64> {
    let mut out = HashMap::new();
    let Some(rt) = ue::try_runtime() else { return out };
    // SAFETY: validated image base + offsets from try_runtime.
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return out;
    }
    for obj in view.iter() {
        let is_mesh = obj
            .class()
            .map(|c| c.as_object().name() == "StaticMesh")
            .unwrap_or(false);
        if is_mesh {
            out.entry(obj.name()).or_insert(obj.as_ptr() as u64);
        }
    }
    out
}

/// Give a freshly begun StaticMeshActor its mesh. Mobility first:
/// a Static component rejects a mesh swap once registered.
fn apply_mesh(actor: u64, mesh_ptr: u64) {
    // SAFETY: actor came from begin_spawn this frame; the
    // component slot is the documented StaticMeshActor field.
    let comp: *const u8 = unsafe { read_at(actor as *const u8, STATIC_MESH_COMPONENT_OFFSET) };
    if comp.is_null() {
        return;
    }
    if let Some(scene) = ue::find_class_fast("SceneComponent") {
        if let Some(set_mobility) = scene.get_function("SceneComponent", "SetMobility") {
            let mut parms = [MOBILITY_MOVABLE];
            // SAFETY: comp is a live USceneComponent; SetMobility
            // takes one byte.
            unsafe {
                (*(comp as *const UObject))
                    .process_event(set_mobility, parms.as_mut_ptr() as *mut std::ffi::c_void);
            }
        }
    }
    let Some(smc) = ue::find_class_fast("StaticMeshComponent") else { return };
    let Some(set_mesh) = smc.get_function("StaticMeshComponent", "SetStaticMesh") else {
        return;
    };
    let mut parms = [0u8; 0x10];
    parms[0x00..0x08].copy_from_slice(&mesh_ptr.to_le_bytes());
    // SAFETY: comp is a live UStaticMeshComponent; SetStaticMesh
    // takes NewMesh at 0x00 with a bool return at 0x08.
    unsafe {
        (*(comp as *const UObject))
            .process_event(set_mesh, parms.as_mut_ptr() as *mut std::ffi::c_void);
    }
}

pub fn register_ops() {
    ueforge::ops::OP_REGISTRY.register_many([
        ueforge::ops::OpDef::new(
            "harvest_classes",
            "Class histogram of the actors in a level",
            "{level: str}",
            harvest_classes,
        ),
        ueforge::ops::OpDef::new(
            "harvest_square",
            "Harvest a level's actors into a reusable composition",
            "{level: str, classes?: [str], centre_x?: f64, centre_y?: f64}",
            harvest_square,
        ),
        ueforge::ops::OpDef::new(
            "compose",
            "Spawn a saved composition (defaults to the player's position)",
            "{composition: obj, x?: f64, y?: f64, z?: f64, yaw?: f64, max?: u64}",
            compose,
        ),
    ]);
}
