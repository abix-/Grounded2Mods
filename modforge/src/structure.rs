//! The structure vocabulary: buildings as DATA. A StructureDef is
//! rooms (with openings and stairs), furniture, and lights; plain
//! glam math and rgb colors, no engine types. Hand-authoring and
//! generation produce the same shape.
//!
//! Consumers own the spawner (the binder): topside turns a def into
//! Bevy entities; a mod would call its host game's build functions.
//! The relation rules live here in [`validate`] so every consumer
//! and every generator shares one idea of a legal building.
//!
//! Coordinates are structure-local: y up, north = negative z. Each
//! room carries its own origin (center of its floor).

use glam::Vec3;

/// Plain linear rgb triplet; consumers convert to their engine's
/// color type.
pub type Rgb = [f32; 3];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Side {
    North,
    South,
    East,
    West,
}

impl Side {
    /// Unit vector pointing out of a room through this side.
    pub fn outward(self) -> Vec3 {
        match self {
            Side::North => Vec3::NEG_Z,
            Side::South => Vec3::Z,
            Side::East => Vec3::X,
            Side::West => Vec3::NEG_X,
        }
    }
}

/// A gap in one wall. `offset` runs along the wall from its center
/// (+x on north/south walls, +z on east/west walls). `sill` lifts
/// the gap's bottom above the room floor (a solid panel fills the
/// wall below it); 0 is a floor-level doorway. With `door`, a
/// hinged door fills the gap; without, it stays open.
///
/// Rule for connected rooms: BOTH rooms author the matching opening
/// in their facing walls.
#[derive(Clone)]
pub struct Opening {
    pub side: Side,
    pub offset: f32,
    pub width: f32,
    pub sill: f32,
    pub door: bool,
}

/// One rectangular room at its own origin: interior size (x width,
/// y height, z length), wall thickness, openings. `floor` and
/// `ceiling` are skipped when another room's slab already covers
/// that face (a stacked room's floor is the room-below's ceiling).
#[derive(Clone)]
pub struct RoomSpec {
    pub origin: Vec3,
    pub interior: Vec3,
    pub wall_thickness: f32,
    pub openings: Vec<Opening>,
    pub floor: bool,
    pub ceiling: bool,
}

/// A straight solid staircase: starts at `base` (floor level,
/// center of the bottom edge), ascends toward `side`, climbing
/// `rise`, with a flat `landing` run at the top. Rule: leave flat
/// approach room in front of the base; stairs must never end at a
/// wall.
#[derive(Clone)]
pub struct StairSpec {
    pub base: Vec3,
    pub side: Side,
    pub width: f32,
    pub rise: f32,
    pub landing: f32,
}

/// A solid block: furniture, a crate, any obstacle. Collides.
#[derive(Clone)]
pub struct SolidSpec {
    pub center: Vec3,
    pub size: Vec3,
    pub color: Rgb,
}

#[derive(Clone)]
pub struct LightSpec {
    pub position: Vec3,
    pub color: Rgb,
    pub intensity: f32,
}

/// A whole building as data.
#[derive(Clone)]
pub struct StructureDef {
    pub name: String,
    pub rooms: Vec<RoomSpec>,
    pub stairs: Vec<StairSpec>,
    pub furniture: Vec<SolidSpec>,
    pub lights: Vec<LightSpec>,
}

/// Axis-aligned box, the shared geometry primitive for interiors,
/// colliders, and walkable surfaces.
#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn from_center_size(center: Vec3, size: Vec3) -> Self {
        Self {
            min: center - size / 2.0,
            max: center + size / 2.0,
        }
    }

    pub fn overlaps(&self, other: &Aabb) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
            && self.min.z < other.max.z
            && self.max.z > other.min.z
    }
}

pub fn room_interior_aabb(room: &RoomSpec) -> Aabb {
    Aabb {
        min: room.origin - Vec3::new(room.interior.x / 2.0, 0.0, room.interior.z / 2.0),
        max: room.origin
            + Vec3::new(room.interior.x / 2.0, room.interior.y, room.interior.z / 2.0),
    }
}

/// The relation rules that make a structure a logical place. Today:
/// room interiors must not overlap (touching wall planes are
/// legal). Reachability, matching openings, and stair clearance
/// join this check when generation starts authoring defs.
pub fn validate(def: &StructureDef) -> Result<(), String> {
    for (i, a) in def.rooms.iter().enumerate() {
        for (j, b) in def.rooms.iter().enumerate().skip(i + 1) {
            if room_interior_aabb(a).overlaps(&room_interior_aabb(b)) {
                return Err(format!(
                    "structure '{}': room {i} and room {j} interiors overlap",
                    def.name
                ));
            }
        }
    }
    Ok(())
}

/// Wall run axis, outward wall-center offset from the room origin,
/// and wall run length for one side of one room. The shared frame
/// math every spawner builds walls from.
pub fn side_frame(side: Side, interior: Vec3, t: f32) -> (Vec3, Vec3, f32) {
    let (w, l) = (interior.x, interior.z);
    match side {
        Side::North => (Vec3::X, Vec3::new(0.0, 0.0, -(l / 2.0 + t / 2.0)), w + 2.0 * t),
        Side::South => (Vec3::X, Vec3::new(0.0, 0.0, l / 2.0 + t / 2.0), w + 2.0 * t),
        Side::East => (Vec3::Z, Vec3::new(w / 2.0 + t / 2.0, 0.0, 0.0), l + 2.0 * t),
        Side::West => (Vec3::Z, Vec3::new(-(w / 2.0 + t / 2.0), 0.0, 0.0), l + 2.0 * t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(origin: Vec3) -> RoomSpec {
        RoomSpec {
            origin,
            interior: Vec3::new(6.0, 3.0, 8.0),
            wall_thickness: 0.2,
            openings: vec![],
            floor: true,
            ceiling: true,
        }
    }

    fn def(rooms: Vec<RoomSpec>) -> StructureDef {
        StructureDef {
            name: "test".to_string(),
            rooms,
            stairs: vec![],
            furniture: vec![],
            lights: vec![],
        }
    }

    #[test]
    fn overlapping_room_interiors_are_rejected() {
        assert!(validate(&def(vec![room(Vec3::ZERO), room(Vec3::new(1.0, 0.0, 0.0))])).is_err());
    }

    #[test]
    fn touching_rooms_are_legal() {
        // Stacked (shared floor/ceiling plane) and side by side
        // (shared wall plane).
        assert!(validate(&def(vec![room(Vec3::ZERO), room(Vec3::new(0.0, 3.0, 0.0))])).is_ok());
        assert!(validate(&def(vec![room(Vec3::ZERO), room(Vec3::new(6.0, 0.0, 0.0))])).is_ok());
    }
}
