//! Building rooms out of a game's own modular kit.
//!
//! Most games that ship modular level parts ship the same kinds:
//! wall segments in a few widths, some with a doorway or a
//! window, and floor tiles. Given a way to name the part that
//! fills a slot, the rest is the same everywhere: work out the
//! shell, name each piece, put them in the world.
//!
//! The shell is [`modforge::structure::room_pieces`]. This adds
//! the Unreal half: reading a level's kit parts, and spawning a
//! room into the world.
//!
//! A consumer supplies [`Kit`] and gets three endpoints. Nothing
//! here knows a mesh name.

use modforge::structure::{KitNames, PieceDef};

/// What a game must say about its modular parts.
///
/// `class` is the actor to spawn per piece; `modules` are the
/// widths the kit offers, largest first; `names` says what the
/// kit calls each part.
#[derive(Clone, Copy)]
pub struct Kit {
    pub class: &'static str,
    pub modules: &'static [f32],
    pub names: KitNames,
    /// Mesh name prefixes that count as kit parts, for reading a
    /// level back (e.g. `["SM_Wall", "SM_Floor"]`).
    pub prefixes: &'static [&'static str],
    /// How far above and below a point to look for the ground.
    pub trace_up: f64,
    pub trace_down: f64,
}

impl Kit {
    fn pieces(&self, room: &modforge::structure::RoomDef) -> Vec<PieceDef> {
        modforge::structure::room_pieces(room, self.modules, self.class, |s| self.names.mesh_for(s))
    }
}

fn piece_json(p: &PieceDef) -> serde_json::Value {
    serde_json::json!({
        "mesh": p.asset,
        "at": [p.offset.x, p.offset.y, p.offset.z],
        "yaw": p.yaw,
    })
}

/// Register the room endpoints for one kit.
///
/// - `room_plan` reports what a room WOULD be built from, without
///   touching the world. The testable half.
/// - `build_room` builds one at a spot.
/// - `kit_layout` reads a level's kit parts back, which is the
///   raw material for learning how the game's own designers
///   assemble rooms.
///
/// All three enter the engine, so a consumer routes them through
/// whatever serves its game thread.
pub fn register_ops(kit: Kit) {
    crate::ops::OP_REGISTRY.register_many([
        crate::ops::OpDef::new(
            "room_plan",
            "What a room would be built from, without building it",
            "{width?, length?, height?, door?, windows?, floor?, ceiling?}",
            move |args| {
                let pieces = kit.pieces(&modforge::structure::room_from_json(args));
                Ok(serde_json::json!({
                    "count": pieces.len(),
                    // This crate's numbers: metres, y up, radians.
                    // Unreal's exist only at the moment of spawning.
                    "pieces": pieces.iter().map(piece_json).collect::<Vec<_>>(),
                }))
            },
        ),
        crate::ops::OpDef::new(
            "build_room",
            "Build a room at a spot, standing on the ground",
            "{x: f64, y: f64, width?, length?, height?, door?, windows?, floor?, ceiling?}",
            move |args| {
                let x = crate::args::arg_f64(args, "x")?;
                let y = crate::args::arg_f64(args, "y")?;
                let pieces = kit.pieces(&modforge::structure::room_from_json(args));
                let z = super::trace::ground_at(x, y, kit.trace_up, kit.trace_down)
                    .ok_or("no ground there")?;
                let world = super::actor::any_world_actor().ok_or("no level loaded")?;
                // SAFETY: world came from the search above; the
                // caller is responsible for the game thread.
                let out = unsafe { super::pieces::spawn(world, &pieces, (x, y, z), 0.0, usize::MAX) };
                Ok(serde_json::json!({
                    "placed": out.placed,
                    "failed": out.failed,
                    "at": [x, y, z],
                }))
            },
        ),
        crate::ops::OpDef::new(
            "kit_layout",
            "Every kit part placed in a level, with position and facing",
            "{level: str}",
            move |args| {
                let level = crate::args::arg_str(args, "level")?.to_string();
                let all = super::pieces::read_level(&level, &[]);
                let rows: Vec<serde_json::Value> = all
                    .iter()
                    .filter(|p| match &p.asset {
                        Some(m) => kit.prefixes.iter().any(|pre| m.starts_with(pre)),
                        None => false,
                    })
                    .map(piece_json)
                    .collect();
                Ok(serde_json::json!({
                    "level": level,
                    "kit_pieces": rows.len(),
                    "total_pieces": all.len(),
                    "pieces": rows,
                }))
            },
        ),
    ]);
}
