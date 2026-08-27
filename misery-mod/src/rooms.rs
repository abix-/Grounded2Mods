//! MISERY's modular kit, as data.
//!
//! Everything about building a room is shared: modforge works out
//! the shell (`room_pieces`), `ueforge::ue::rooms` owns the
//! endpoints and the spawning. This file only says what MISERY
//! calls its parts.
//!
//! Kit facts, all measured live (worldgen.md 9.5):
//! - A wall is named `SM_Wall_<width>x<height>` in centimetres,
//!   and those numbers ARE its size.
//! - Its marker sits at the bottom of its starting edge, geometry
//!   running +x for width and +z for height, centred in
//!   thickness. So a wall is placed AT the corner it starts from,
//!   not at its middle.
//! - Floor tiles are marked at a corner with the walking surface
//!   at marker height, so a floor is placed at floor level.

use modforge::structure::{KitNames, SlotOpening};

/// Module widths the kit offers, largest first, in metres.
pub const MODULES: &[f32] = &[4.0, 2.0, 1.0];

/// What MISERY calls its parts.
///
/// The `openings` list is what the kit actually ships. A size not
/// listed gets a solid wall instead of a hole.
///
/// Two entries need explaining:
/// - A wall 4 m tall is named `400x401`, not `400x400`.
/// - 4 m doorways use the 3 m door. `SM_WallDoor_400x400` is the
///   kit's one malformed part: 458x56x460 with an off-centre
///   marker.
pub const NAMES: KitNames = KitNames {
    wall: "SM_Wall",
    floor: "SM_Floor",
    units_per_metre: 100.0,
    openings: &[
        (SlotOpening::Door, 400, 300, "SM_WallDoor_400x300"),
        (SlotOpening::Door, 400, 400, "SM_WallDoor_400x300"),
        (SlotOpening::Door, 200, 400, "SM_WallDoor_200x400"),
        (SlotOpening::Window, 400, 300, "SM_WallWindow_400x300"),
        (SlotOpening::Window, 400, 400, "SM_WallWindow_400x300"),
        (SlotOpening::Window, 200, 400, "SM_WallWindowSmall_200x400"),
    ],
    walls: &[(400, 400, "SM_Wall_400x401")],
};

pub const KIT: ueforge::ue::rooms::Kit = ueforge::ue::rooms::Kit {
    class: "StaticMeshActor",
    modules: MODULES,
    names: NAMES,
    prefixes: &["SM_Wall", "SM_Floor"],
    trace_up: crate::TRACE_UP,
    trace_down: crate::TRACE_DOWN,
};

/// Adds room-building controls using the wall and floor pieces shipped with MISERY.
/// Stays here because the kit names and trace distances are game content; Modforge plans rooms and Ueforge spawns them.
pub fn register_ops() {
    ueforge::ue::rooms::register_ops(KIT);
}
