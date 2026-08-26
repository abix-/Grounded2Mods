//! The kit binder: turns modforge shell slots into MISERY meshes.
//!
//! modforge decides what a room needs (`shell_slots`: floor tiles,
//! wall segments per side, which segment carries the door). This
//! module knows only how MISERY names and pivots its parts
//! (worldgen.md 9.5), and builds the pieces to spawn.
//!
//! Kit facts this depends on, all measured live:
//! - A wall mesh is named `SM_Wall_<width>x<height>` in cm and
//!   those numbers ARE its size.
//! - Its pivot is the bottom of its starting edge, geometry
//!   running +x for width and +z for height, centred in
//!   thickness. So a wall is placed AT the corner it starts from,
//!   not at its middle.
//! - Floor tiles pivot at a corner with the walking surface at
//!   pivot height, so a floor is placed at floor level.

use glam::Vec3;
use modforge::structure::{
    Opening, RoomDef, ShellSlot, Side, SlotKind, SlotOpening, shell_slots,
};

use modforge::structure::PieceDef;

/// Module widths the kit offers, largest first, in metres.
pub const MODULES: &[f32] = &[4.0, 2.0, 1.0];

/// A wall 4 m tall is named 400x401, not 400x400.
pub fn wall_name(w: i32, h: i32) -> String {
    if w == 400 && h == 400 {
        return "SM_Wall_400x401".to_string();
    }
    format!("SM_Wall_{w}x{h}")
}

/// The mesh for one wall segment. Falls back to a plain wall when
/// the kit has no door or window at that size, so a room is never
/// left with a hole.
pub fn wall_mesh(width_cm: i32, height_cm: i32, opening: Option<SlotOpening>) -> String {
    match opening {
        // SM_WallDoor_400x400 is the kit's one outlier (458x56x460
        // with an off-centre pivot), so 4 m doorways use the 3 m
        // door rather than the broken piece.
        Some(SlotOpening::Door) => match (width_cm, height_cm) {
            (400, 300) | (400, 400) => "SM_WallDoor_400x300".to_string(),
            (200, 400) => "SM_WallDoor_200x400".to_string(),
            _ => wall_name(width_cm, height_cm),
        },
        Some(SlotOpening::Window) => match (width_cm, height_cm) {
            (400, 300) | (400, 400) => "SM_WallWindow_400x300".to_string(),
            (200, 400) => "SM_WallWindowSmall_200x400".to_string(),
            _ => wall_name(width_cm, height_cm),
        },
        None => wall_name(width_cm, height_cm),
    }
}

/// The mesh for one floor or ceiling tile.
pub fn floor_mesh(width_cm: i32, depth_cm: i32) -> String {
    let (a, b) = if width_cm <= depth_cm {
        (width_cm, depth_cm)
    } else {
        (depth_cm, width_cm)
    };
    format!("SM_Floor_{a}x{b}")
}

/// Round a metre span to the kit's centimetre naming.
fn cm(v: f32) -> i32 {
    (v * 100.0).round() as i32
}

/// Build the pieces for one room, in modforge's space, ready for
/// `ueforge::ue::pieces::spawn`.
///
/// This module's only job is naming: which mesh fills each slot.
/// Positions and facings pass through untouched, and the two
/// conventions are reconciled once, on the way into the world.
pub fn room_pieces(room: &RoomDef) -> Vec<PieceDef> {
    let mut out = Vec::new();
    for slot in shell_slots(room, MODULES) {
        let mesh = match slot.kind {
            SlotKind::Wall => wall_mesh(cm(slot.width), cm(slot.height), slot.opening),
            SlotKind::Floor | SlotKind::Ceiling => floor_mesh(cm(slot.width), cm(slot.height)),
        };
        out.push(to_piece(&slot, mesh));
    }
    out
}

/// A room described over the wire: interior size in metres, wall
/// height, and which sides get a door or a window.
fn room_from_args(args: &serde_json::Value) -> RoomDef {
    let f = |k: &str, d: f64| args.get(k).and_then(|v| v.as_f64()).unwrap_or(d) as f32;
    let width = f("width", 8.0);
    let length = f("length", 8.0);
    let height = f("height", 3.0);
    let mut openings = Vec::new();
    // A door on the south wall and a window on each other side:
    // enough to prove openings land in the right segments.
    if args.get("door").and_then(|v| v.as_bool()).unwrap_or(true) {
        openings.push(Opening {
            side: Side::South,
            offset: 0.0,
            width: 1.2,
            sill: 0.0,
            door: true,
        });
    }
    if args.get("windows").and_then(|v| v.as_bool()).unwrap_or(true) {
        for side in [Side::North, Side::East, Side::West] {
            openings.push(Opening {
                side,
                offset: 0.0,
                width: 1.0,
                sill: 1.0,
                door: false,
            });
        }
    }
    RoomDef {
        origin: Vec3::ZERO,
        interior: Vec3::new(width, height, length),
        wall_thickness: 0.2,
        openings,
        floor: args.get("floor").and_then(|v| v.as_bool()).unwrap_or(true),
        ceiling: args.get("ceiling").and_then(|v| v.as_bool()).unwrap_or(false),
    }
}

/// What a room WOULD be built from: the mesh list with offsets,
/// without spawning anything. The binder's testable surface.
fn room_plan(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let room = room_from_args(args);
    let pieces = room_pieces(&room);
    Ok(serde_json::json!({
        "count": pieces.len(),
        // modforge's numbers: metres, y up, radians. The Unreal
        // ones only exist at the moment of spawning.
        "pieces": pieces.iter().map(|p| serde_json::json!({
            "mesh": p.asset,
            "at": [p.offset.x, p.offset.y, p.offset.z],
            "yaw": p.yaw,
        })).collect::<Vec<_>>(),
    }))
}

/// Build a room in the world at the player's feet.
fn build_room(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let room = room_from_args(args);
    let pieces = room_pieces(&room);
    let away = args.get("away").and_then(|v| v.as_f64()).unwrap_or(1200.0);
    crate::dispatch::DRAIN.queue().enqueue(
        move || {
            let player = ueforge::ue::actor::find_actors_by_chain("BP_SGKMasterCharacter_C")
                .into_iter()
                .next()
                .ok_or("no player")?;
            // SAFETY: live player actor, game thread.
            let here = unsafe { ueforge::ue::transform::world_location(player) }
                .ok_or("no location")?;
            // In front of the player, not along a fixed compass
            // direction: UE yaw 0 faces +x, and yaw grows toward +y.
            // SAFETY: as above.
            let yaw = unsafe { ueforge::ue::transform::read(player) }
                .map(|t| t.yaw)
                .unwrap_or(0.0)
                .to_radians();
            let (x, y) = (here.0 + yaw.cos() * away, here.1 + yaw.sin() * away);
            let z = ueforge::ue::trace::ground_at(x, y, crate::TRACE_UP, crate::TRACE_DOWN)
                .unwrap_or(here.2);
            // SAFETY: player is a live actor, game thread.
            let out = unsafe {
                ueforge::ue::pieces::spawn(player, &pieces, (x, y, z), 0.0, usize::MAX)
            };
            Ok(serde_json::json!({
                "placed": out.placed,
                "failed": out.failed,
                "at": [x, y, z],
            }))
        },
        std::time::Duration::from_secs(30),
    )
}

/// Every kit piece placed in a level, with where it sits and
/// which way it faces. The raw material for understanding how
/// the level designers actually build rooms.
fn kit_layout(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let level = args
        .get("level")
        .and_then(|v| v.as_str())
        .ok_or("need {level: str}")?;
    let pieces = ueforge::ue::pieces::read_level(level, &[]);
    let mut rows = Vec::new();
    for p in &pieces {
        let Some(mesh) = &p.asset else { continue };
        // Kit parts only: the modular walls, floors and openings.
        if !(mesh.starts_with("SM_Wall") || mesh.starts_with("SM_Floor")) {
            continue;
        }
        rows.push(serde_json::json!({
            "mesh": mesh,
            "x": p.offset.x,
            "y": p.offset.y,
            "z": p.offset.z,
            "yaw": p.yaw,
        }));
    }
    Ok(serde_json::json!({
        "level": level,
        "kit_pieces": rows.len(),
        "total_pieces": pieces.len(),
        "pieces": rows,
    }))
}

pub fn register_ops() {
    ueforge::ops::OP_REGISTRY.register_many([
        ueforge::ops::OpDef::new(
            "kit_layout",
            "Every modular kit piece placed in a level, with position and yaw",
            "{level: str}",
            kit_layout,
        ),
        ueforge::ops::OpDef::new(
            "room_plan",
            "The mesh list a room would be built from, without spawning",
            "{width?, length?, height?, door?, windows?, floor?, ceiling?}",
            room_plan,
        ),
        ueforge::ops::OpDef::new(
            "build_room",
            "Build a generated room in front of the player",
            "{width?, length?, height?, away?, door?, windows?, floor?, ceiling?}",
            build_room,
        ),
    ]);
}

/// One slot to one piece.
///
/// No conversion here any more. A `ShellSlot` is already in
/// modforge's space and so is a `PieceDef`, so this only names
/// the mesh. `ueforge::ue::pieces::spawn` turns modforge's
/// numbers into Unreal's once, on the way into the world, which
/// is also the only place the handedness flip is handled.
fn to_piece(slot: &ShellSlot, mesh: String) -> PieceDef {
    PieceDef {
        class: "StaticMeshActor".to_string(),
        asset: Some(mesh),
        offset: slot.position,
        yaw: slot.yaw,
        pitch: 0.0,
        roll: 0.0,
        scale: 1.0,
        // The builder needs no measurements; the mesh carries its
        // own geometry.
        extent: glam::Vec3::ZERO,
    }
}
