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

use modforge::structure::{ShellSlot, SlotKind, SlotOpening};

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

/// MISERY's kit, handed to `ueforge::ue::rooms`.
///
/// Naming is all this game contributes; the shell, the endpoints
/// and the spawning are shared.
pub const KIT: ueforge::ue::rooms::Kit = ueforge::ue::rooms::Kit {
    class: "StaticMeshActor",
    modules: MODULES,
    mesh_for: mesh_for,
    prefixes: &["SM_Wall", "SM_Floor"],
    trace_up: crate::TRACE_UP,
    trace_down: crate::TRACE_DOWN,
};

/// Which mesh fills one slot.
fn mesh_for(slot: &ShellSlot) -> String {
    match slot.kind {
        SlotKind::Wall => wall_mesh(cm(slot.width), cm(slot.height), slot.opening),
        SlotKind::Floor | SlotKind::Ceiling => floor_mesh(cm(slot.width), cm(slot.height)),
    }
}

pub fn register_ops() {
    ueforge::ue::rooms::register_ops(KIT);
}
