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

use crate::combat::{Health, Hit, HitResult, Protection, resolve_hit};

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

/// A whole building as data. `wall_color` paints walls, ceilings,
/// and stairs; `floor_color` the floor slabs.
#[derive(Clone)]
pub struct StructureDef {
    pub name: String,
    pub wall_color: Rgb,
    pub floor_color: Rgb,
    pub rooms: Vec<RoomSpec>,
    pub stairs: Vec<StairSpec>,
    pub furniture: Vec<SolidSpec>,
    pub lights: Vec<LightSpec>,
}

/// The bare concrete every hand-authored building used before
/// colours were data.
pub const CONCRETE_WALL: Rgb = [0.45, 0.45, 0.47];
pub const CONCRETE_FLOOR: Rgb = [0.35, 0.35, 0.36];

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
    // Touching planes computed two ways (k*h + h against (k+1)*h)
    // differ by rounding; shrink each interior by a hair so touching
    // never reads as overlapping.
    const HAIR: f32 = 1e-3;
    let interior = |room: &RoomSpec| {
        let a = room_interior_aabb(room);
        Aabb {
            min: a.min + Vec3::splat(HAIR),
            max: a.max - Vec3::splat(HAIR),
        }
    };
    for (i, a) in def.rooms.iter().enumerate() {
        for (j, b) in def.rooms.iter().enumerate().skip(i + 1) {
            if interior(a).overlaps(&interior(b)) {
                return Err(format!(
                    "structure '{}': room {i} and room {j} interiors overlap",
                    def.name
                ));
            }
        }
    }
    Ok(())
}

/// Floor and landing slab thickness, step depth, and the most one
/// step may rise. Shared by the piece builder and the generators.
pub const SLAB: f32 = 0.1;
pub const STEP_DEPTH: f32 = 0.3;
pub const STEP_RISE_MAX: f32 = 0.31;
/// Health of a freshly built piece until grades land.
pub const PIECE_HEALTH: f32 = 100.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PieceKind {
    Wall,
    Floor,
    Ceiling,
    Step,
    Landing,
    Furniture,
}

/// One solid box of a built structure: a wall segment, a slab, a
/// step, a piece of furniture. The unit of damage and rebuild, the
/// way Rust (the game) treats building blocks. Structure-local
/// centre; the consumer bakes the whole list into one collider and
/// one mesh per colour, and rebakes when the list changes.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub kind: PieceKind,
    pub center: Vec3,
    pub size: Vec3,
    pub color: Rgb,
    pub health: f32,
}

impl Piece {
    pub fn aabb(&self) -> Aabb {
        Aabb::from_center_size(self.center, self.size)
    }
}

/// Every solid box of a structure, from its def: wall segments
/// between openings, panels under sills, floor and ceiling slabs,
/// steps and landings, furniture. Doors are not pieces; they move,
/// and the consumer spawns them from the openings.
pub fn pieces_of(def: &StructureDef) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut push = |kind, center, size, color| {
        pieces.push(Piece {
            kind,
            center,
            size,
            color,
            health: PIECE_HEALTH,
        });
    };
    for room in &def.rooms {
        let t = room.wall_thickness;
        let height = room.interior.y;
        for side in [Side::North, Side::South, Side::East, Side::West] {
            let (axis, frame, length) = side_frame(side, room.interior, t);
            let wall_center = room.origin + frame;
            let mut cuts: Vec<(f32, f32)> = room
                .openings
                .iter()
                .filter(|o| o.side == side)
                .map(|o| (o.offset - o.width / 2.0, o.offset + o.width / 2.0))
                .collect();
            cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut cursor = -length / 2.0;
            let mut segments = Vec::new();
            for (start, end) in cuts {
                if start > cursor {
                    segments.push((cursor, start));
                }
                cursor = cursor.max(end);
            }
            if cursor < length / 2.0 {
                segments.push((cursor, length / 2.0));
            }
            for (start, end) in segments {
                let mid = (start + end) / 2.0;
                let run = end - start;
                let center = wall_center + axis * mid + Vec3::Y * (height / 2.0);
                let size = axis * run + Vec3::Y * height + (Vec3::ONE - axis - Vec3::Y) * t;
                push(PieceKind::Wall, center, size, def.wall_color);
            }
            // Panels below raised sills.
            for opening in room
                .openings
                .iter()
                .filter(|o| o.side == side && o.sill > 0.0)
            {
                let center = wall_center + axis * opening.offset + Vec3::Y * (opening.sill / 2.0);
                let size = axis * opening.width
                    + Vec3::Y * opening.sill
                    + (Vec3::ONE - axis - Vec3::Y) * t;
                push(PieceKind::Wall, center, size, def.wall_color);
            }
        }
        let slab_xz = Vec3::new(room.interior.x + 2.0 * t, 0.0, room.interior.z + 2.0 * t);
        if room.floor {
            push(
                PieceKind::Floor,
                room.origin + Vec3::Y * (SLAB / 2.0),
                slab_xz + Vec3::Y * SLAB,
                def.floor_color,
            );
        }
        if room.ceiling {
            push(
                PieceKind::Ceiling,
                room.origin + Vec3::Y * (height + SLAB),
                slab_xz + Vec3::Y * 0.2,
                def.wall_color,
            );
        }
    }
    for stair in &def.stairs {
        let dir = stair.side.outward();
        let across = Vec3::ONE - dir.abs() - Vec3::Y;
        let steps = (stair.rise / STEP_RISE_MAX).ceil().max(1.0) as usize;
        let step_rise = stair.rise / steps as f32;
        for i in 0..steps {
            let height = step_rise * (i + 1) as f32;
            push(
                PieceKind::Step,
                stair.base + dir * (STEP_DEPTH * (i as f32 + 0.5)) + Vec3::Y * (height / 2.0),
                dir.abs() * STEP_DEPTH + across * stair.width + Vec3::Y * height,
                def.floor_color,
            );
        }
        if stair.landing > 0.0 {
            push(
                PieceKind::Landing,
                stair.base
                    + dir * (STEP_DEPTH * steps as f32 + stair.landing / 2.0)
                    + Vec3::Y * (stair.rise - SLAB / 2.0),
                dir.abs() * stair.landing + across * stair.width + Vec3::Y * SLAB,
                def.floor_color,
            );
        }
    }
    for piece in &def.furniture {
        push(PieceKind::Furniture, piece.center, piece.size, piece.color);
    }
    pieces
}

/// The piece containing a structure-local point, if any: the hit to
/// piece lookup. A point on a shared face belongs to the first piece
/// in list order.
pub fn piece_at(pieces: &[Piece], point: Vec3) -> Option<usize> {
    const SKIN: f32 = 1e-3;
    pieces.iter().position(|p| {
        let a = p.aabb();
        point.x >= a.min.x - SKIN
            && point.x <= a.max.x + SKIN
            && point.y >= a.min.y - SKIN
            && point.y <= a.max.y + SKIN
            && point.z >= a.min.z - SKIN
            && point.z <= a.max.z + SKIN
    })
}

/// Land `hit` on piece `index` through the one damage function
/// (a piece is a target with health and no armor). A piece at zero
/// health is removed; `removed` tells the consumer to rebake.
pub fn damage(pieces: &mut Vec<Piece>, index: usize, hit: &Hit<'_>) -> Option<PieceHit> {
    let piece = pieces.get_mut(index)?;
    let mut health = Health {
        current: piece.health,
        max: PIECE_HEALTH,
    };
    let result = resolve_hit(hit, &mut Protection::default(), &mut health);
    piece.health = health.current;
    let removed = result.killed;
    if removed {
        pieces.remove(index);
    }
    Some(PieceHit { result, removed })
}

/// What a hit did to a piece.
#[derive(Clone, Debug, PartialEq)]
pub struct PieceHit {
    pub result: HitResult,
    pub removed: bool,
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

/// A member structure placed within a monument: a StructureDef at a
/// position relative to the monument origin.
#[derive(Clone)]
pub struct MonumentMember {
    pub structure: StructureDef,
    pub offset: Vec3,
}

/// A spot where loot spawns within the monument. Position is
/// relative to the monument origin. `danger` scales the loot table
/// (icarus difficulty: higher danger, better drops).
#[derive(Clone)]
pub struct LootSpot {
    pub position: Vec3,
    pub danger: u32,
}

/// A spot where an NPC spawns within the monument. Position is
/// relative to the monument origin. `danger` scales the trait pool.
#[derive(Clone)]
pub struct NpcSpot {
    pub position: Vec3,
    pub danger: u32,
}

/// A gate: a locked area requiring progression to access. The gate
/// blocks passage until the player meets the requirement.
#[derive(Clone)]
pub struct Gate {
    pub position: Vec3,
    pub level: u32,
}

/// A solid prop standing in a monument (a car hull, a tent, a
/// barrel): a coloured box. Minor sites are mostly props.
#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    /// Centre of the box, relative to the monument origin.
    pub position: Vec3,
    pub size: Vec3,
    pub color: Rgb,
}

/// One monument as data: a destination worth traveling to, composed
/// of member structures with loot, NPCs, gates, and props.
/// spawn_monument is the one path; members spawn only through
/// spawn_structure. A minor site has no members: props and a loot
/// spot.
#[derive(Clone)]
pub struct MonumentDef {
    pub name: String,
    pub members: Vec<MonumentMember>,
    pub loot_spots: Vec<LootSpot>,
    pub npc_spots: Vec<NpcSpot>,
    pub gates: Vec<Gate>,
    pub props: Vec<Prop>,
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
            wall_color: CONCRETE_WALL,
            floor_color: CONCRETE_FLOOR,
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

    #[test]
    fn a_room_with_a_doorway_becomes_seven_pieces_and_a_hit_finds_the_wall() {
        let mut r = room(Vec3::ZERO);
        r.openings.push(Opening {
            side: Side::North,
            offset: 0.0,
            width: 1.2,
            sill: 0.0,
            door: true,
        });
        let d = def(vec![r]);
        let mut pieces = pieces_of(&d);
        // Three whole walls, two segments beside the doorway, floor,
        // ceiling.
        assert_eq!(pieces.len(), 7);
        assert_eq!(pieces.iter().filter(|p| p.kind == PieceKind::Wall).count(), 5);

        // A point in the east wall band (x 3.0 to 3.2).
        let hit = piece_at(&pieces, Vec3::new(3.1, 1.0, 0.0)).expect("the east wall");
        assert_eq!(pieces[hit].kind, PieceKind::Wall);
        // The doorway gap has no piece.
        assert!(piece_at(&pieces, Vec3::new(0.0, 1.0, -4.1)).is_none());

        // Damage short of death keeps it; death removes it. The
        // piece goes through the one damage function.
        let def = crate::combat::DamageDef {
            name: "test".to_string(),
            amount: 40.0,
            kind: crate::combat::DamageType::Blunt,
            knockback: 0.0,
            ignores_armor: false,
            self_scale: 1.0,
            falloff: crate::combat::Falloff::NONE,
        };
        let swing = Hit {
            def: &def,
            self_inflicted: false,
            distance: 1.0,
            location_scale: 1.0,
        };
        let first = damage(&mut pieces, hit, &swing).unwrap();
        assert!(!first.removed);
        assert_eq!(first.result.damage_dealt, 40.0);
        assert_eq!(pieces.len(), 7);
        assert!(!damage(&mut pieces, hit, &swing).unwrap().removed);
        assert!(damage(&mut pieces, hit, &swing).unwrap().removed, "120 of 100 health");
        assert_eq!(pieces.len(), 6);
        assert!(piece_at(&pieces, Vec3::new(3.1, 1.0, 0.0)).is_none());
        assert!(damage(&mut pieces, 99, &swing).is_none());
    }

    #[test]
    fn monument_def_composes_structures() {
        let warehouse = def(vec![room(Vec3::ZERO)]);
        let shack = def(vec![room(Vec3::ZERO)]);
        let monument = MonumentDef {
            name: "roadside stop".to_string(),
            members: vec![
                MonumentMember { structure: warehouse, offset: Vec3::ZERO },
                MonumentMember { structure: shack, offset: Vec3::new(15.0, 0.0, 0.0) },
            ],
            loot_spots: vec![
                LootSpot { position: Vec3::new(0.0, 0.3, 0.0), danger: 1 },
                LootSpot { position: Vec3::new(15.0, 0.3, 0.0), danger: 1 },
            ],
            npc_spots: vec![
                NpcSpot { position: Vec3::new(7.0, 0.0, 3.0), danger: 1 },
            ],
            gates: vec![],
            props: vec![],
        };
        assert_eq!(monument.members.len(), 2);
        assert_eq!(monument.loot_spots.len(), 2);
        assert_eq!(monument.npc_spots.len(), 1);
        assert!(monument.gates.is_empty());
    }
}
