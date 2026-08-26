//! Where an actor is: read its transform, and the mesh it shows.
//!
//! Every UE actor reaches its transform the same way, through
//! `AActor::RootComponent` to a `USceneComponent`. None of this
//! is game-specific, so it belongs here rather than in a mod: any
//! mod that places, moves, measures, or copies anything needs it.
//!
//! UE 5 stores `FVector` and `FRotator` as doubles (large world
//! coordinates), so these are `f64`. On UE 4 they are `f32` and
//! the offsets differ; this module is UE 5.
//!
//! Offsets are engine layouts read from the object dump:
//!
//! ```text
//! AActor::RootComponent            +0x1A0
//! USceneComponent::RelativeLocation +0x128   FVector  (3 x f64)
//! USceneComponent::RelativeRotation +0x140   FRotator (pitch, yaw, roll)
//! USceneComponent::RelativeScale3D  +0x158   FVector
//! ```

use std::ffi::c_void;

use super::{UObject, read_at};

pub mod offsets {
    /// `AActor::RootComponent`.
    pub const ROOT_COMPONENT: usize = 0x1A0;
    /// `USceneComponent::RelativeLocation`.
    pub const RELATIVE_LOCATION: usize = 0x128;
    /// `USceneComponent::RelativeRotation`. FRotator is stored
    /// pitch, yaw, roll, NOT the order it reads in Blueprint.
    pub const RELATIVE_ROTATION: usize = 0x140;
    /// `USceneComponent::RelativeScale3D`.
    pub const RELATIVE_SCALE_3D: usize = 0x158;
    /// `AStaticMeshActor::StaticMeshComponent`.
    pub const STATIC_MESH_COMPONENT: usize = 0x290;
    /// `UStaticMeshComponent::StaticMesh`, the asset itself.
    pub const STATIC_MESH: usize = 0x560;
    /// `UStaticMesh::ExtendedBounds`, an `FBoxSphereBounds`
    /// `{ Origin, BoxExtent, SphereRadius }`.
    pub const EXTENDED_BOUNDS: usize = 0x1F0;
    /// `BoxExtent` within `FBoxSphereBounds`: the HALF-size of
    /// the mesh's own geometry.
    pub const BOX_EXTENT: usize = 0x18;
}

/// `EComponentMobility`. A Static component cannot be moved at
/// runtime, so anything spawned to be positioned must be Movable
/// first.
pub const MOBILITY_STATIC: u8 = 0;
pub const MOBILITY_STATIONARY: u8 = 1;
pub const MOBILITY_MOVABLE: u8 = 2;

/// An actor's placement, as the engine stores it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Transform {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Degrees, UE convention.
    pub pitch: f64,
    pub yaw: f64,
    pub roll: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub scale_z: f64,
}

/// The actor's root `USceneComponent`, or `None` if it has none.
///
/// # Safety
/// `actor` must be a live `AActor`.
pub unsafe fn root_component(actor: *const u8) -> Option<*const u8> {
    // SAFETY: caller guarantees a live AActor; RootComponent is a
    // stable field across UE 5.x.
    let root: *const u8 = unsafe { read_at(actor, offsets::ROOT_COMPONENT) };
    if root.is_null() { None } else { Some(root) }
}

/// Read an actor's transform through its root component.
///
/// # Safety
/// `actor` must be a live `AActor`.
pub unsafe fn read(actor: *const u8) -> Option<Transform> {
    // SAFETY: caller guarantees a live AActor.
    let root = unsafe { root_component(actor)? };
    // SAFETY: root is a live USceneComponent and the three
    // transform fields sit at documented offsets within it.
    unsafe {
        Some(Transform {
            x: read_at(root, offsets::RELATIVE_LOCATION),
            y: read_at(root, offsets::RELATIVE_LOCATION + 8),
            z: read_at(root, offsets::RELATIVE_LOCATION + 16),
            pitch: read_at(root, offsets::RELATIVE_ROTATION),
            yaw: read_at(root, offsets::RELATIVE_ROTATION + 8),
            roll: read_at(root, offsets::RELATIVE_ROTATION + 16),
            scale_x: read_at(root, offsets::RELATIVE_SCALE_3D),
            scale_y: read_at(root, offsets::RELATIVE_SCALE_3D + 8),
            scale_z: read_at(root, offsets::RELATIVE_SCALE_3D + 16),
        })
    }
}

/// The actor's WORLD location, via `Actor:K2_GetActorLocation`.
///
/// Different from [`read`], which returns the root component's
/// RELATIVE transform: relative to its parent, which for an actor
/// attached to something else is not where it is in the world.
/// Use this when the answer has to be a world position; use
/// [`read`] when copying a placement, since that is what a
/// harvest wants to reproduce.
///
/// Goes through ProcessEvent, so game thread only.
///
/// # Safety
/// `actor` must be a live `AActor`.
pub unsafe fn world_location(actor: *const u8) -> Option<(f64, f64, f64)> {
    let cls = super::find_class_fast("Actor")?;
    let func = cls.get_function("Actor", "K2_GetActorLocation")?;
    let mut parms = [0f64; 3];
    // SAFETY: caller guarantees a live actor on the game thread;
    // the function returns one FVector, which is this buffer.
    unsafe {
        (*(actor as *const UObject)).process_event(func, parms.as_mut_ptr() as *mut c_void);
    }
    Some((parms[0], parms[1], parms[2]))
}

/// A static mesh's name and the HALF-size of its own geometry.
///
/// Read off the asset, not the actor, so it is the mesh's
/// intrinsic size before any scaling. That is what lets a piece
/// be classified by shape.
///
/// # Safety
/// `mesh` must be a live `UStaticMesh`.
pub unsafe fn mesh_extent(mesh: *const u8) -> (f64, f64, f64) {
    let at = offsets::EXTENDED_BOUNDS + offsets::BOX_EXTENT;
    // SAFETY: caller guarantees a live UStaticMesh; ExtendedBounds
    // is an inline FBoxSphereBounds.
    unsafe {
        (
            read_at::<f64>(mesh, at),
            read_at::<f64>(mesh, at + 8),
            read_at::<f64>(mesh, at + 16),
        )
    }
}

/// The mesh a `AStaticMeshActor` is showing: its name and its
/// half-extent. `None` when the actor has no mesh component or no
/// asset on it.
///
/// # Safety
/// `actor` must be a live `AActor`.
pub unsafe fn static_mesh(actor: *const u8) -> Option<(String, f64, f64, f64)> {
    // SAFETY: caller guarantees a live actor; the chain is
    // StaticMeshActor -> its component -> the asset.
    unsafe {
        let comp: *const u8 = read_at(actor, offsets::STATIC_MESH_COMPONENT);
        if comp.is_null() {
            return None;
        }
        let mesh: *const u8 = read_at(comp, offsets::STATIC_MESH);
        let name = (mesh as *const UObject).as_ref()?.name();
        let (ex, ey, ez) = mesh_extent(mesh);
        Some((name, ex, ey, ez))
    }
}

/// Every loaded static mesh, by name.
///
/// Built in one pass. A caller placing many pieces resolves each
/// mesh from this map instead of searching the object list per
/// piece.
///
/// Only meshes currently IN MEMORY appear. What a game ships is a
/// larger set, and comes from the asset registry
/// (`ueforge::assets`) rather than from here.
pub fn loaded_meshes() -> std::collections::HashMap<String, u64> {
    let mut out = std::collections::HashMap::new();
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
        if is_mesh {
            out.entry(obj.name()).or_insert(obj.as_ptr() as u64);
        }
    }
    out
}

/// A mesh's own geometry: where its box sits relative to its
/// position marker, and its half-size.
///
/// The marker offset decides all placement maths. A wall whose
/// marker is at its base corner is laid differently from one
/// whose marker is in the middle, and the difference is invisible
/// until things sit in the wrong place.
///
/// # Safety
/// `mesh` must be a live `UStaticMesh`.
pub unsafe fn mesh_bounds(mesh: *const u8) -> ((f64, f64, f64), (f64, f64, f64)) {
    let at = offsets::EXTENDED_BOUNDS;
    // SAFETY: caller guarantees a live UStaticMesh; ExtendedBounds
    // is an inline FBoxSphereBounds { Origin, BoxExtent, Radius }.
    unsafe {
        let origin = (
            read_at::<f64>(mesh, at),
            read_at::<f64>(mesh, at + 8),
            read_at::<f64>(mesh, at + 16),
        );
        (origin, mesh_extent(mesh))
    }
}

/// Give a freshly begun static mesh actor its mesh.
///
/// Mobility first: a component spawned Static silently rejects
/// both a mesh swap and every later attempt to move it, so the
/// order here is not cosmetic.
///
/// Call between `spawn::begin_spawn` and `spawn::finish_spawn`,
/// on the game thread.
///
/// # Safety
/// `actor` must be a live actor and `mesh` a live `UStaticMesh`.
pub unsafe fn set_actor_mesh(actor: u64, mesh: u64) {
    // SAFETY: caller guarantees a live actor.
    let Some(comp) = (unsafe { static_mesh_component(actor as *const u8) }) else {
        return;
    };
    // SAFETY: comp is this actor's live component, game thread.
    let _ = unsafe { set_mobility(comp, MOBILITY_MOVABLE) };
    // SAFETY: as above; mesh is a live UStaticMesh.
    let _ = unsafe { set_static_mesh(comp, mesh) };
}

/// The actor's static mesh COMPONENT, for callers that need to
/// write to it.
///
/// # Safety
/// `actor` must be a live `AActor`.
pub unsafe fn static_mesh_component(actor: *const u8) -> Option<*const u8> {
    // SAFETY: caller guarantees a live actor.
    let comp: *const u8 = unsafe { read_at(actor, offsets::STATIC_MESH_COMPONENT) };
    if comp.is_null() { None } else { Some(comp) }
}

/// Make a component movable (or static, or stationary).
///
/// A component spawned as Static ignores every attempt to move
/// it, silently. Game thread only.
///
/// # Safety
/// `comp` must be a live `USceneComponent`.
pub unsafe fn set_mobility(comp: *const u8, mobility: u8) -> Result<(), &'static str> {
    let cls = super::find_class_fast("SceneComponent").ok_or("SceneComponent class not loaded")?;
    let f = cls
        .get_function("SceneComponent", "SetMobility")
        .ok_or("SceneComponent has no SetMobility")?;
    let mut parms = [mobility];
    // SAFETY: caller guarantees a live component on the game
    // thread; SetMobility takes one byte.
    unsafe {
        (*(comp as *const UObject)).process_event(f, parms.as_mut_ptr() as *mut c_void);
    }
    Ok(())
}

/// Point a static mesh component at a mesh asset. Game thread
/// only.
///
/// # Safety
/// `comp` must be a live `UStaticMeshComponent` and `mesh` a live
/// `UStaticMesh`.
pub unsafe fn set_static_mesh(comp: *const u8, mesh: u64) -> Result<(), &'static str> {
    let cls = super::find_class_fast("StaticMeshComponent")
        .ok_or("StaticMeshComponent class not loaded")?;
    let f = cls
        .get_function("StaticMeshComponent", "SetStaticMesh")
        .ok_or("StaticMeshComponent has no SetStaticMesh")?;
    // NewMesh at 0x00, bool return at 0x08.
    let mut parms = [0u8; 0x10];
    parms[0x00..0x08].copy_from_slice(&mesh.to_le_bytes());
    // SAFETY: caller guarantees a live component on the game
    // thread; the parm block matches the function's layout.
    unsafe {
        (*(comp as *const UObject)).process_event(f, parms.as_mut_ptr() as *mut c_void);
    }
    Ok(())
}
