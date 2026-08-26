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
pub struct RoomDef {
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
pub struct StairDef {
    pub base: Vec3,
    pub side: Side,
    pub width: f32,
    pub rise: f32,
    pub landing: f32,
}

/// A solid block: furniture, a crate, any obstacle. Collides.
#[derive(Clone)]
pub struct SolidDef {
    pub center: Vec3,
    pub size: Vec3,
    pub color: Rgb,
}

#[derive(Clone)]
pub struct LightDef {
    pub position: Vec3,
    pub color: Rgb,
    pub intensity: f32,
}

/// One placed piece of a CAPTURED structure: an opaque host-game
/// asset plus where it sits relative to the structure origin.
///
/// Authored structures describe themselves as rooms; a structure
/// captured out of a running game cannot, because its buildings
/// are prefab meshes with no room data to read. Both are still
/// structures: one coherent thing you place in the world. The
/// consumer's spawner resolves `class` and `asset` however its
/// host game names things (a UE class name and a mesh name, a
/// prefab id, a blueprint path).
///
/// Offsets follow the structure convention: y up, north = -z.
/// Consumers on z-up engines convert in their binder.
#[derive(Clone, Debug, PartialEq)]
pub struct PieceDef {
    /// What to spawn, in the host game's naming.
    pub class: String,
    /// A second identity for classes that are a shell around an
    /// asset (a static mesh actor and the mesh it carries).
    pub asset: Option<String>,
    pub offset: Vec3,
    /// Turn about the up axis, radians.
    pub yaw: f32,
    /// Tilt, radians. Roofs and ramps need it; most scenery is 0.
    pub pitch: f32,
    pub roll: f32,
    pub scale: f32,
    /// Half-size of the piece's own geometry in its local axes,
    /// metres. `Vec3::ZERO` when the host could not measure it.
    /// This is what makes a piece more than a name: with it, a
    /// piece is a box in space and its role can be read off its
    /// proportions.
    pub extent: Vec3,
}

/// What a piece is, judged by its proportions alone. No names, no
/// per-game knowledge: a thin tall wide box is a wall whatever
/// the game calls it.
///
/// Read in the structure convention (y up), so "flat" means small
/// in y and "thin" means small in one horizontal axis.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PieceShape {
    /// Flat and wide: floors, ceilings, road slabs, platforms.
    Slab,
    /// Thin in one horizontal axis, long and tall: walls, fences.
    Panel,
    /// Narrow in both horizontal axes, tall: posts, pillars, poles.
    Post,
    /// Long in one horizontal axis, small in the other two: beams,
    /// pipes, rails.
    Beam,
    /// Roughly equal in all three: crates, rocks, machinery.
    Block,
    /// Too small to be architecture: clutter and props.
    Clutter,
    /// No measurement available.
    Unknown,
}

/// Anything smaller than this in every axis is clutter, not
/// architecture. Metres (half-extent).
const CLUTTER_HALF_SIZE: f32 = 0.35;

/// Classify a piece by the proportions of its box.
///
/// The thresholds are ratios, not sizes, so the same rules read a
/// garden fence and a factory wall the same way.
pub fn classify(extent: Vec3) -> PieceShape {
    let (x, y, z) = (extent.x.abs(), extent.y.abs(), extent.z.abs());
    if x <= 0.0 && y <= 0.0 && z <= 0.0 {
        return PieceShape::Unknown;
    }
    if x < CLUTTER_HALF_SIZE && y < CLUTTER_HALF_SIZE && z < CLUTTER_HALF_SIZE {
        return PieceShape::Clutter;
    }
    let horizontal_max = x.max(z);
    let horizontal_min = x.min(z);
    // Flat: height is a small fraction of the ground span.
    if y * 4.0 < horizontal_max && horizontal_min * 3.0 > horizontal_max {
        return PieceShape::Slab;
    }
    // Thin in one horizontal axis but tall and long: a panel.
    if horizontal_min * 4.0 < horizontal_max && y * 2.0 > horizontal_max {
        return PieceShape::Panel;
    }
    // Narrow footprint, tall: a post.
    if y > horizontal_max * 2.0 {
        return PieceShape::Post;
    }
    // Long and slender horizontally: a beam.
    if horizontal_max > horizontal_min * 4.0 && horizontal_max > y * 2.0 {
        return PieceShape::Beam;
    }
    PieceShape::Block
}

impl PieceDef {
    /// This piece's shape, from its measured extent.
    pub fn shape(&self) -> PieceShape {
        classify(self.extent * self.scale.abs())
    }

    /// Where this piece ends up when its whole set is placed at
    /// `origin` and turned by `turn` radians about the up axis.
    ///
    /// Returns the position and the piece's own facing, both
    /// already turned. A set of pieces keeps its shape because
    /// every offset turns by the same angle and every facing
    /// gains it.
    ///
    /// Offsets are stored relative to the set's middle, which is
    /// what lets a captured or authored set be put down anywhere.
    /// Doing this by hand per consumer is how one caller ends up
    /// turning the offsets but forgetting the facings, and the
    /// walls face outward.
    pub fn placed_at(&self, origin: Vec3, turn: f32) -> (Vec3, f32) {
        let (s, c) = turn.sin_cos();
        let o = self.offset;
        (
            Vec3::new(
                origin.x + o.x * c - o.z * s,
                origin.y + o.y,
                origin.z + o.x * s + o.z * c,
            ),
            self.yaw + turn,
        )
    }

    /// World-space half-extent ignoring tilt: the box a piece
    /// occupies on the ground, with yaw applied.
    pub fn ground_half_size(&self) -> (f32, f32) {
        let e = self.extent * self.scale.abs();
        let (s, c) = self.yaw.sin_cos();
        (
            (e.x * c).abs() + (e.z * s).abs(),
            (e.x * s).abs() + (e.z * c).abs(),
        )
    }
}

/// Cut a loose heap of pieces into groups that stand together.
///
/// A level read as pieces is one flat list: a building, its
/// fence, the junk pile beside it, and a rock forty metres away,
/// all mixed. Things that belong together are near each other, so
/// grouping by distance recovers them without knowing anything
/// about the game.
///
/// `radius` is how far apart two pieces can be and still count as
/// one thing, in metres on the ground plane. Height is ignored:
/// an upper floor belongs with the floor beneath it.
///
/// Groups smaller than `min` are noise and are dropped. Over
/// `max` only the seed is dropped and the rest are reconsidered,
/// so a dense area yields several smaller things rather than
/// nothing at all.
///
/// Each group comes back re-centred on its own middle, with its
/// lowest point at y = 0, so it can be placed anywhere and turned
/// about a sensible pivot.
pub fn group_nearby(
    pieces: &[PieceDef],
    radius: f32,
    min: usize,
    max: usize,
) -> Vec<Vec<PieceDef>> {
    let mut taken = vec![false; pieces.len()];
    let mut out = Vec::new();
    for i in 0..pieces.len() {
        if taken[i] {
            continue;
        }
        let seed = &pieces[i];
        let mut members: Vec<usize> = Vec::new();
        for (j, other) in pieces.iter().enumerate() {
            if taken[j] {
                continue;
            }
            let dx = other.offset.x - seed.offset.x;
            let dz = other.offset.z - seed.offset.z;
            if dx * dx + dz * dz <= radius * radius {
                members.push(j);
            }
            if members.len() > max {
                break;
            }
        }
        if members.len() < min || members.len() > max {
            // Mark only the seed: the others may still group with
            // something else.
            taken[i] = true;
            continue;
        }
        let n = members.len() as f32;
        let cx = members.iter().map(|&j| pieces[j].offset.x).sum::<f32>() / n;
        let cz = members.iter().map(|&j| pieces[j].offset.z).sum::<f32>() / n;
        let base_y = members
            .iter()
            .map(|&j| pieces[j].offset.y)
            .fold(f32::MAX, f32::min);
        out.push(
            members
                .iter()
                .map(|&j| PieceDef {
                    offset: pieces[j].offset - Vec3::new(cx, base_y, cz),
                    ..pieces[j].clone()
                })
                .collect(),
        );
        for &j in &members {
            taken[j] = true;
        }
    }
    out
}

/// Turn a heap of loose pieces into named structures.
///
/// [`group_nearby`] with the naming and colouring done, which is
/// what a caller wants when capturing a place: it has a source
/// name and a palette, not a pile of groups.
pub fn capture(
    source: &str,
    pieces: &[PieceDef],
    radius: f32,
    min: usize,
    max: usize,
    wall_color: Rgb,
    floor_color: Rgb,
) -> Vec<StructureDef> {
    group_nearby(pieces, radius, min, max)
        .into_iter()
        .map(|group| StructureDef {
            name: source.to_string(),
            wall_color,
            floor_color,
            rooms: Vec::new(),
            stairs: Vec::new(),
            furniture: Vec::new(),
            lights: Vec::new(),
            pieces: group,
        })
        .collect()
}

/// A collection of structures to draw from.
///
/// Holds a bounded number and forgets the oldest beyond that, so
/// a long session cannot grow it without limit. Drawing is with
/// replacement: the same structure can appear twice in one draw,
/// which is what makes a row of similar buildings possible.
#[derive(Default, Clone)]
pub struct Library {
    items: Vec<StructureDef>,
    cap: usize,
}

impl Library {
    pub fn new(cap: usize) -> Self {
        Self {
            items: Vec::new(),
            cap,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Add a structure, dropping the oldest if that would go over
    /// the cap.
    pub fn add(&mut self, s: StructureDef) {
        if self.cap == 0 {
            return;
        }
        self.items.push(s);
        while self.items.len() > self.cap {
            self.items.remove(0);
        }
    }

    /// What the library currently holds, oldest first. For
    /// reporting what has been collected; drawing from it is
    /// [`draw`].
    ///
    /// [`draw`]: Library::draw
    pub fn items(&self) -> &[StructureDef] {
        &self.items
    }

    /// Keep the `n` biggest of `structures` and discard the rest.
    ///
    /// Biggest by piece count: a capture of one place yields both
    /// a building and the litter around it, and the building is
    /// the part worth keeping.
    pub fn add_best(&mut self, mut structures: Vec<StructureDef>, n: usize) -> usize {
        structures.sort_by_key(|s| std::cmp::Reverse(s.pieces.len()));
        structures.truncate(n);
        let kept = structures.len();
        for s in structures {
            self.add(s);
        }
        kept
    }

    /// Draw somewhere between `lo` and `hi` at random.
    pub fn draw_between(&self, lo: usize, hi: usize) -> Vec<StructureDef> {
        let hi = hi.max(lo);
        self.draw(lo + fastrand::usize(0..=(hi - lo)))
    }

    /// Draw `n` at random, with replacement. Empty when the
    /// library is empty.
    pub fn draw(&self, n: usize) -> Vec<StructureDef> {
        if self.items.is_empty() {
            return Vec::new();
        }
        (0..n)
            .map(|_| self.items[fastrand::usize(0..self.items.len())].clone())
            .collect()
    }
}

/// A whole building as data. `wall_color` paints walls, ceilings,
/// and stairs; `floor_color` the floor slabs.
///
/// A structure is authored (rooms, stairs, furniture, lights) or
/// captured (`pieces`, lifted from a running game), or both. The
/// generic machinery (footprints, arrangement, monuments) works
/// over either, so a game that cannot author rooms still gets
/// generated places.
#[derive(Clone)]
pub struct StructureDef {
    pub name: String,
    pub wall_color: Rgb,
    pub floor_color: Rgb,
    pub rooms: Vec<RoomDef>,
    pub stairs: Vec<StairDef>,
    pub furniture: Vec<SolidDef>,
    pub lights: Vec<LightDef>,
    /// Captured pieces. Empty for authored structures.
    pub pieces: Vec<PieceDef>,
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

pub fn room_interior_aabb(room: &RoomDef) -> Aabb {
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
    let interior = |room: &RoomDef| {
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
    /// The rolled name ("Miller's Stop").
    pub name: String,
    /// The monument type it rolled from ("roadside stop"): what a
    /// person remembers it as, so the registry can say what such a
    /// place is worth.
    pub kind: String,
    pub members: Vec<MonumentMember>,
    pub loot_spots: Vec<LootSpot>,
    pub npc_spots: Vec<NpcSpot>,
    pub gates: Vec<Gate>,
    pub props: Vec<Prop>,
    /// What the place is good for to someone who knows it, from its
    /// type (life.md).
    pub good_for: crate::memory::GoodFor,
}

/// What a shell slot is for. A builder supplies one piece per
/// slot from whatever kit its host game has.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlotKind {
    Floor,
    Wall,
    Ceiling,
}

/// What a wall slot must contain, if anything.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlotOpening {
    Door,
    Window,
}

/// One piece of a room's shell that a builder must supply: where
/// it goes, how big a span it must cover, and what it needs to
/// contain.
///
/// `position` is the slot's own origin in structure-local space
/// (y up): for a wall, the bottom of its starting edge; for a
/// floor or ceiling tile, its corner. `yaw` turns the slot about
/// the up axis, so a wall runs along +x before rotation.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ShellSlot {
    pub kind: SlotKind,
    pub position: Vec3,
    pub yaw: f32,
    /// Span the piece must cover: width along the slot's run, and
    /// height (walls) or depth (floors and ceilings).
    pub width: f32,
    pub height: f32,
    pub opening: Option<SlotOpening>,
}

/// Greedy fill of a run using the largest modules that fit, so a
/// 7 m wall becomes 4 + 2 + 1 when the kit has those sizes.
/// `modules` must be sorted largest first. Returns each piece's
/// start offset along the run and its width.
pub fn fill_run(run: f32, modules: &[f32]) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let mut at = 0.0f32;
    // A hair of tolerance so float remainders do not spawn a
    // sliver piece at the end of every wall.
    const EPS: f32 = 1e-3;
    'fill: while run - at > EPS {
        let left = run - at;
        for &m in modules {
            if m <= left + EPS {
                out.push((at, m));
                at += m;
                continue 'fill;
            }
        }
        // Nothing fits the remainder; stop rather than overhang.
        break;
    }
    out
}

/// Decompose a room into the shell pieces a builder must supply:
/// floor tiles, wall segments on all four sides with openings
/// assigned, and ceiling tiles.
///
/// `modules` are the kit's available widths, largest first (e.g.
/// `[4.0, 2.0, 1.0]`). Walls are laid on the room's interior
/// boundary; a segment whose span overlaps an opening's span
/// carries that opening.
pub fn shell_slots(room: &RoomDef, modules: &[f32]) -> Vec<ShellSlot> {
    let mut out = Vec::new();
    let (w, h, l) = (room.interior.x, room.interior.y, room.interior.z);
    let half = Vec3::new(w / 2.0, 0.0, l / 2.0);
    let corner = room.origin - half;

    if room.floor {
        for (x, dx) in fill_run(w, modules) {
            for (z, dz) in fill_run(l, modules) {
                out.push(ShellSlot {
                    kind: SlotKind::Floor,
                    position: corner + Vec3::new(x, 0.0, z),
                    yaw: 0.0,
                    width: dx,
                    height: dz,
                    opening: None,
                });
            }
        }
    }
    if room.ceiling {
        for (x, dx) in fill_run(w, modules) {
            for (z, dz) in fill_run(l, modules) {
                out.push(ShellSlot {
                    kind: SlotKind::Ceiling,
                    position: corner + Vec3::new(x, h, z),
                    yaw: 0.0,
                    width: dx,
                    height: dz,
                    opening: None,
                });
            }
        }
    }

    // Each side: where its run starts, which way it runs, and the
    // run's length. Walls run along +x before yaw is applied.
    let sides = [
        (Side::South, corner + Vec3::new(0.0, 0.0, l), 0.0f32, w),
        (Side::North, corner + Vec3::new(w, 0.0, 0.0), std::f32::consts::PI, w),
        (
            Side::West,
            corner,
            -std::f32::consts::FRAC_PI_2,
            l,
        ),
        (
            Side::East,
            corner + Vec3::new(w, 0.0, l),
            std::f32::consts::FRAC_PI_2,
            l,
        ),
    ];

    for (side, start, yaw, run) in sides {
        let (s, c) = yaw.sin_cos();
        for (at, width) in fill_run(run, modules) {
            // A segment claims the opening whose centre falls in
            // it. Openings are usually narrower than a module (a
            // 1.2 m door in a 4 m wall), so comparing spans would
            // never match; the piece that contains the doorway is
            // the one that must carry it.
            let opening = room
                .openings
                .iter()
                .filter(|o| o.side == side)
                .find(|o| {
                    let centre = run / 2.0 + o.offset;
                    centre >= at && centre < at + width
                })
                .map(|o| {
                    if o.door {
                        SlotOpening::Door
                    } else {
                        SlotOpening::Window
                    }
                });
            out.push(ShellSlot {
                kind: SlotKind::Wall,
                position: start + Vec3::new(at * c, 0.0, -at * s),
                yaw,
                width,
                height: h,
                opening,
            });
        }
    }
    out
}

#[cfg(test)]
mod shell_tests {
    use super::*;

    fn room(w: f32, h: f32, l: f32, openings: Vec<Opening>) -> RoomDef {
        RoomDef {
            origin: Vec3::ZERO,
            interior: Vec3::new(w, h, l),
            wall_thickness: 0.2,
            openings,
            floor: true,
            ceiling: true,
        }
    }

    #[test]
    fn fill_run_uses_largest_modules_first() {
        assert_eq!(fill_run(8.0, &[4.0, 2.0, 1.0]), vec![(0.0, 4.0), (4.0, 4.0)]);
        assert_eq!(
            fill_run(7.0, &[4.0, 2.0, 1.0]),
            vec![(0.0, 4.0), (4.0, 2.0), (6.0, 1.0)]
        );
    }

    #[test]
    fn fill_run_stops_rather_than_overhanging() {
        // 3 m of run with only 4 m modules: nothing fits.
        assert!(fill_run(3.0, &[4.0]).is_empty());
    }

    #[test]
    fn a_plain_room_has_four_walls_of_segments() {
        let r = room(8.0, 3.0, 8.0, vec![]);
        let slots = shell_slots(&r, &[4.0, 2.0, 1.0]);
        let walls: Vec<&ShellSlot> =
            slots.iter().filter(|s| s.kind == SlotKind::Wall).collect();
        // 8 m per side at 4 m modules = 2 segments, four sides.
        assert_eq!(walls.len(), 8, "expected 8 wall segments");
        assert!(walls.iter().all(|s| (s.height - 3.0).abs() < 1e-4));
    }

    #[test]
    fn floor_and_ceiling_tile_the_interior() {
        let r = room(8.0, 3.0, 4.0, vec![]);
        let slots = shell_slots(&r, &[4.0]);
        let floors = slots.iter().filter(|s| s.kind == SlotKind::Floor).count();
        let ceils = slots.iter().filter(|s| s.kind == SlotKind::Ceiling).count();
        // 8x4 at 4 m tiles = 2 tiles each.
        assert_eq!(floors, 2);
        assert_eq!(ceils, 2);
    }

    #[test]
    fn a_door_claims_exactly_one_segment() {
        let r = room(
            8.0,
            3.0,
            8.0,
            vec![Opening {
                side: Side::South,
                offset: -2.0,
                width: 1.2,
                sill: 0.0,
                door: true,
            }],
        );
        let slots = shell_slots(&r, &[4.0]);
        let doors: Vec<&ShellSlot> = slots
            .iter()
            .filter(|s| s.opening == Some(SlotOpening::Door))
            .collect();
        assert_eq!(doors.len(), 1, "one door segment, got {}", doors.len());
        assert_eq!(doors[0].kind, SlotKind::Wall);
    }

    #[test]
    fn a_window_is_distinct_from_a_door() {
        let r = room(
            4.0,
            3.0,
            4.0,
            vec![Opening {
                side: Side::North,
                offset: 0.0,
                width: 1.0,
                sill: 1.0,
                door: false,
            }],
        );
        let slots = shell_slots(&r, &[4.0]);
        assert_eq!(
            slots
                .iter()
                .filter(|s| s.opening == Some(SlotOpening::Window))
                .count(),
            1
        );
        assert_eq!(
            slots
                .iter()
                .filter(|s| s.opening == Some(SlotOpening::Door))
                .count(),
            0
        );
    }

    #[test]
    fn a_floorless_roofless_room_is_walls_only() {
        let mut r = room(4.0, 3.0, 4.0, vec![]);
        r.floor = false;
        r.ceiling = false;
        let slots = shell_slots(&r, &[4.0]);
        assert!(slots.iter().all(|s| s.kind == SlotKind::Wall));
        assert_eq!(slots.len(), 4);
    }

    #[test]
    fn wall_segments_start_on_the_interior_boundary() {
        let r = room(4.0, 3.0, 4.0, vec![]);
        let slots = shell_slots(&r, &[4.0]);
        for s in slots.iter().filter(|s| s.kind == SlotKind::Wall) {
            // Every wall start sits on the interior rectangle.
            let on_x = (s.position.x.abs() - 2.0).abs() < 1e-3;
            let on_z = (s.position.z.abs() - 2.0).abs() < 1e-3;
            assert!(
                on_x || on_z,
                "wall at {:?} is not on the boundary",
                s.position
            );
        }
    }
}

#[cfg(test)]
mod shape_tests {
    use super::*;

    /// Half-extents in metres, structure convention (y up).
    fn e(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3::new(x, y, z)
    }

    #[test]
    fn floor_slab_is_a_slab() {
        // 8m x 8m floor, 20cm thick.
        assert_eq!(classify(e(4.0, 0.1, 4.0)), PieceShape::Slab);
    }

    #[test]
    fn wall_panel_is_a_panel() {
        // 4m long, 20cm thick, 3m tall.
        assert_eq!(classify(e(2.0, 1.5, 0.1)), PieceShape::Panel);
        // Same wall turned the other way.
        assert_eq!(classify(e(0.1, 1.5, 2.0)), PieceShape::Panel);
    }

    #[test]
    fn pillar_is_a_post() {
        // 40cm square, 3m tall.
        assert_eq!(classify(e(0.2, 1.5, 0.2)), PieceShape::Post);
    }

    #[test]
    fn pipe_is_a_beam() {
        // 6m long, 30cm through.
        assert_eq!(classify(e(3.0, 0.15, 0.15)), PieceShape::Beam);
    }

    #[test]
    fn crate_is_a_block() {
        assert_eq!(classify(e(0.6, 0.6, 0.6)), PieceShape::Block);
    }

    #[test]
    fn small_prop_is_clutter() {
        assert_eq!(classify(e(0.1, 0.2, 0.15)), PieceShape::Clutter);
    }

    #[test]
    fn unmeasured_is_unknown() {
        assert_eq!(classify(Vec3::ZERO), PieceShape::Unknown);
    }

    #[test]
    fn scale_is_applied_before_classifying() {
        // A clutter-sized box scaled up ten times is architecture.
        let p = PieceDef {
            class: "x".into(),
            asset: None,
            offset: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            scale: 10.0,
            extent: e(0.2, 0.15, 0.02),
        };
        assert_eq!(p.shape(), PieceShape::Panel);
    }

    fn at(x: f32, y: f32, z: f32) -> PieceDef {
        PieceDef {
            class: "x".into(),
            asset: None,
            offset: Vec3::new(x, y, z),
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            scale: 1.0,
            extent: e(0.5, 0.5, 0.5),
        }
    }

    /// Two clusters ten metres apart, with a radius of three, are
    /// two things and not one.
    #[test]
    fn nearby_pieces_group_and_distant_ones_do_not() {
        let mut pieces = Vec::new();
        for i in 0..4 {
            pieces.push(at(i as f32 * 0.5, 0.0, 0.0));
        }
        for i in 0..4 {
            pieces.push(at(10.0 + i as f32 * 0.5, 0.0, 0.0));
        }
        let groups = group_nearby(&pieces, 3.0, 2, 100);
        assert_eq!(groups.len(), 2, "got {} group(s)", groups.len());
        assert_eq!(groups[0].len(), 4);
        assert_eq!(groups[1].len(), 4);
    }

    #[test]
    fn a_group_below_the_minimum_is_dropped() {
        let pieces: Vec<PieceDef> = (0..3).map(|i| at(i as f32 * 0.1, 0.0, 0.0)).collect();
        assert!(group_nearby(&pieces, 1.0, 5, 100).is_empty());
    }

    /// Over the maximum, only the seed is dropped and the rest
    /// regroup. A dense area yields smaller things rather than
    /// nothing at all.
    #[test]
    fn an_oversized_group_regroups_smaller() {
        let pieces: Vec<PieceDef> = (0..3).map(|i| at(i as f32 * 0.1, 0.0, 0.0)).collect();
        let groups = group_nearby(&pieces, 1.0, 1, 2);
        assert!(!groups.is_empty(), "a dense area should still yield something");
        assert!(
            groups.iter().all(|g| g.len() <= 2),
            "every group must respect the maximum"
        );
    }

    /// Height must not split a group, or an upper floor becomes
    /// its own building.
    #[test]
    fn height_does_not_separate_pieces() {
        let pieces = vec![at(0.0, 0.0, 0.0), at(0.5, 40.0, 0.0)];
        let groups = group_nearby(&pieces, 3.0, 2, 100);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    /// A group comes back centred on itself, sitting on y = 0, so
    /// it can be put down anywhere.
    #[test]
    fn a_group_is_recentred_on_its_own_middle() {
        let pieces = vec![at(100.0, 5.0, 100.0), at(102.0, 7.0, 100.0)];
        let groups = group_nearby(&pieces, 5.0, 2, 100);
        assert_eq!(groups.len(), 1);
        let g = &groups[0];
        let mid_x: f32 = g.iter().map(|p| p.offset.x).sum::<f32>() / g.len() as f32;
        let mid_z: f32 = g.iter().map(|p| p.offset.z).sum::<f32>() / g.len() as f32;
        let low_y = g.iter().map(|p| p.offset.y).fold(f32::MAX, f32::min);
        assert!(mid_x.abs() < 1e-4, "x middle was {mid_x}");
        assert!(mid_z.abs() < 1e-4, "z middle was {mid_z}");
        assert!(low_y.abs() < 1e-4, "lowest point was {low_y}");
    }

    #[test]
    fn no_piece_lands_in_two_groups() {
        let pieces: Vec<PieceDef> = (0..12).map(|i| at(i as f32 * 0.4, 0.0, 0.0)).collect();
        let groups = group_nearby(&pieces, 1.0, 2, 100);
        let total: usize = groups.iter().map(|g| g.len()).sum();
        assert!(total <= pieces.len(), "{total} placed from {}", pieces.len());
    }

    fn structure(name: &str) -> StructureDef {
        StructureDef {
            name: name.into(),
            wall_color: CONCRETE_WALL,
            floor_color: CONCRETE_FLOOR,
            rooms: Vec::new(),
            stairs: Vec::new(),
            furniture: Vec::new(),
            lights: Vec::new(),
            pieces: Vec::new(),
        }
    }

    #[test]
    fn a_library_forgets_the_oldest_past_its_cap() {
        let mut lib = Library::new(3);
        for i in 0..5 {
            lib.add(structure(&format!("s{i}")));
        }
        assert_eq!(lib.len(), 3);
        // The first two are gone, the last three remain.
        let drawn = lib.draw(50);
        assert!(drawn.iter().all(|s| s.name != "s0" && s.name != "s1"));
    }

    #[test]
    fn an_empty_library_draws_nothing() {
        assert!(Library::new(10).draw(5).is_empty());
        // A cap of zero holds nothing at all.
        let mut none = Library::new(0);
        none.add(structure("s"));
        assert!(none.is_empty());
    }

    #[test]
    fn add_best_keeps_the_biggest_and_drops_the_rest() {
        let mut lib = Library::new(10);
        let mut small = structure("small");
        small.pieces = vec![at(0.0, 0.0, 0.0)];
        let mut big = structure("big");
        big.pieces = (0..9).map(|i| at(i as f32, 0.0, 0.0)).collect();
        let mut middling = structure("middling");
        middling.pieces = (0..4).map(|i| at(i as f32, 0.0, 0.0)).collect();

        let kept = lib.add_best(vec![small, big, middling], 2);
        assert_eq!(kept, 2);
        let names: Vec<&str> = lib.items().iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"big") && names.contains(&"middling"));
        assert!(!names.contains(&"small"), "the litter should be dropped");
    }

    #[test]
    fn add_best_with_fewer_than_asked_keeps_them_all() {
        let mut lib = Library::new(10);
        assert_eq!(lib.add_best(vec![structure("a")], 5), 1);
    }

    #[test]
    fn draw_between_stays_within_its_bounds() {
        let mut lib = Library::new(10);
        lib.add(structure("a"));
        for _ in 0..200 {
            let n = lib.draw_between(2, 5).len();
            assert!((2..=5).contains(&n), "drew {n}");
        }
        // Reversed bounds must not panic.
        assert_eq!(lib.draw_between(4, 2).len(), 4);
    }

    #[test]
    fn capture_names_and_colours_every_group() {
        let pieces: Vec<PieceDef> = (0..4).map(|i| at(i as f32 * 0.5, 0.0, 0.0)).collect();
        let got = capture("a_square", &pieces, 3.0, 2, 100, CONCRETE_WALL, CONCRETE_FLOOR);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "a_square");
        assert_eq!(got[0].wall_color, CONCRETE_WALL);
        assert_eq!(got[0].pieces.len(), 4);
    }

    #[test]
    fn a_draw_returns_what_was_asked_for() {
        let mut lib = Library::new(10);
        lib.add(structure("only"));
        let drawn = lib.draw(4);
        // With replacement, so one structure can fill a draw of
        // four; that is what makes a row of similar buildings.
        assert_eq!(drawn.len(), 4);
        assert!(drawn.iter().all(|s| s.name == "only"));
    }

    #[test]
    fn ground_half_size_turns_with_yaw() {
        let p = PieceDef {
            class: "x".into(),
            asset: None,
            offset: Vec3::ZERO,
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: 0.0,
            roll: 0.0,
            scale: 1.0,
            extent: e(2.0, 1.5, 0.1),
        };
        let (hx, hz) = p.ground_half_size();
        // Turned 90 degrees, the long axis now runs the other way.
        assert!((hx - 0.1).abs() < 1e-4, "hx was {hx}");
        assert!((hz - 2.0).abs() < 1e-4, "hz was {hz}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(origin: Vec3) -> RoomDef {
        RoomDef {
            origin,
            interior: Vec3::new(6.0, 3.0, 8.0),
            wall_thickness: 0.2,
            openings: vec![],
            floor: true,
            ceiling: true,
        }
    }

    fn def(rooms: Vec<RoomDef>) -> StructureDef {
        StructureDef {
            name: "test".to_string(),
            wall_color: CONCRETE_WALL,
            floor_color: CONCRETE_FLOOR,
            rooms,
            stairs: vec![],
            furniture: vec![],
            lights: vec![],
            pieces: vec![],
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
            kind: "roadside stop".to_string(),
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
            good_for: Default::default(),
        };
        assert_eq!(monument.members.len(), 2);
        assert_eq!(monument.loot_spots.len(), 2);
        assert_eq!(monument.npc_spots.len(), 1);
        assert!(monument.gates.is_empty());
    }
}
