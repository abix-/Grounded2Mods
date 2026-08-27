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

/// One placed part of a CAPTURED structure: an opaque host-game
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
pub struct PartDef {
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
    /// Half-size of the part's own geometry in its local axes,
    /// metres. `Vec3::ZERO` when the host could not measure it.
    /// This is what makes a part more than a name: with it, a
    /// part is a box in space and its role can be read off its
    /// proportions.
    pub extent: Vec3,
}

/// What a part is, judged by its proportions alone. No names, no
/// per-game knowledge: a thin tall wide box is a wall whatever
/// the game calls it.
///
/// Read in the structure convention (y up), so "flat" means small
/// in y and "thin" means small in one horizontal axis.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PartShape {
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

/// Classify a part by the proportions of its box.
///
/// The thresholds are ratios, not sizes, so the same rules read a
/// garden fence and a factory wall the same way.
pub fn classify(extent: Vec3) -> PartShape {
    let (x, y, z) = (extent.x.abs(), extent.y.abs(), extent.z.abs());
    if x <= 0.0 && y <= 0.0 && z <= 0.0 {
        return PartShape::Unknown;
    }
    if x < CLUTTER_HALF_SIZE && y < CLUTTER_HALF_SIZE && z < CLUTTER_HALF_SIZE {
        return PartShape::Clutter;
    }
    let horizontal_max = x.max(z);
    let horizontal_min = x.min(z);
    // Flat: height is a small fraction of the ground span.
    if y * 4.0 < horizontal_max && horizontal_min * 3.0 > horizontal_max {
        return PartShape::Slab;
    }
    // Thin in one horizontal axis but tall and long: a panel.
    if horizontal_min * 4.0 < horizontal_max && y * 2.0 > horizontal_max {
        return PartShape::Panel;
    }
    // Narrow footprint, tall: a post.
    if y > horizontal_max * 2.0 {
        return PartShape::Post;
    }
    // Long and slender horizontally: a beam.
    if horizontal_max > horizontal_min * 4.0 && horizontal_max > y * 2.0 {
        return PartShape::Beam;
    }
    PartShape::Block
}

impl PartDef {
    /// This part's shape, from its measured extent.
    pub fn shape(&self) -> PartShape {
        classify(self.extent * self.scale.abs())
    }

    /// Where this part ends up when its whole set is placed at
    /// `origin` and turned by `turn` radians about the up axis.
    ///
    /// Returns the position and the part's own facing, both
    /// already turned. A set of parts keeps its shape because
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

    /// World-space half-extent ignoring tilt: the box a part
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

/// Cut a loose heap of parts into groups that stand together.
///
/// A level read as parts is one flat list: a building, its
/// fence, the junk pile beside it, and a rock forty metres away,
/// all mixed. Things that belong together are near each other, so
/// grouping by distance recovers them without knowing anything
/// about the game.
///
/// `radius` is how far apart two parts can be and still count as
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
    parts: &[PartDef],
    radius: f32,
    min: usize,
    max: usize,
) -> Vec<Vec<PartDef>> {
    let mut taken = vec![false; parts.len()];
    let mut out = Vec::new();
    for i in 0..parts.len() {
        if taken[i] {
            continue;
        }
        let seed = &parts[i];
        let mut members: Vec<usize> = Vec::new();
        for (j, other) in parts.iter().enumerate() {
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
        let cx = members.iter().map(|&j| parts[j].offset.x).sum::<f32>() / n;
        let cz = members.iter().map(|&j| parts[j].offset.z).sum::<f32>() / n;
        let base_y = members
            .iter()
            .map(|&j| parts[j].offset.y)
            .fold(f32::MAX, f32::min);
        out.push(
            members
                .iter()
                .map(|&j| PartDef {
                    offset: parts[j].offset - Vec3::new(cx, base_y, cz),
                    ..parts[j].clone()
                })
                .collect(),
        );
        for &j in &members {
            taken[j] = true;
        }
    }
    out
}

/// Turn a heap of loose parts into named structures.
///
/// [`group_nearby`] with the naming and colouring done, which is
/// what a caller wants when capturing a place: it has a source
/// name and a palette, not a pile of groups.
pub fn capture(
    source: &str,
    parts: &[PartDef],
    radius: f32,
    min: usize,
    max: usize,
    wall_color: Rgb,
    floor_color: Rgb,
) -> Vec<StructureDef> {
    group_nearby(parts, radius, min, max)
        .into_iter()
        .map(|group| StructureDef {
            name: source.to_string(),
            wall_color,
            floor_color,
            rooms: Vec::new(),
            stairs: Vec::new(),
            furniture: Vec::new(),
            lights: Vec::new(),
            parts: group,
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
    /// Biggest by part count: a capture of one place yields both
    /// a building and the litter around it, and the building is
    /// the part worth keeping.
    pub fn add_best(&mut self, mut structures: Vec<StructureDef>, n: usize) -> usize {
        structures.sort_by_key(|s| std::cmp::Reverse(s.parts.len()));
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
/// captured (`parts`, lifted from a running game), or both. The
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
    /// Captured parts. Empty for authored structures.
    pub parts: Vec<PartDef>,
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
/// step may rise. Shared by the part builder and the generators.
pub const SLAB: f32 = 0.1;
pub const STEP_DEPTH: f32 = 0.3;
pub const STEP_RISE_MAX: f32 = 0.31;
/// Health of a freshly built part until grades land.
pub const PART_HEALTH: f32 = 100.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PartKind {
    Wall,
    Floor,
    Ceiling,
    Step,
    Landing,
    Furniture,
}

/// One solid box of a built structure: a wall segment, a slab, a
/// step, a part of furniture. The unit of damage and rebuild, the
/// way Rust (the game) treats building blocks. Structure-local
/// centre; the consumer bakes the whole list into one collider and
/// one mesh per colour, and rebakes when the list changes.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub kind: PartKind,
    pub center: Vec3,
    pub size: Vec3,
    pub color: Rgb,
    pub health: f32,
}

impl Part {
    pub fn aabb(&self) -> Aabb {
        Aabb::from_center_size(self.center, self.size)
    }
}

/// Every solid box of a structure, from its def: wall segments
/// between openings, panels under sills, floor and ceiling slabs,
/// steps and landings, furniture. Doors are not parts; they move,
/// and the consumer spawns them from the openings.
pub fn parts_of(def: &StructureDef) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut push = |kind, center, size, color| {
        parts.push(Part {
            kind,
            center,
            size,
            color,
            health: PART_HEALTH,
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
                push(PartKind::Wall, center, size, def.wall_color);
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
                push(PartKind::Wall, center, size, def.wall_color);
            }
        }
        let slab_xz = Vec3::new(room.interior.x + 2.0 * t, 0.0, room.interior.z + 2.0 * t);
        if room.floor {
            push(
                PartKind::Floor,
                room.origin + Vec3::Y * (SLAB / 2.0),
                slab_xz + Vec3::Y * SLAB,
                def.floor_color,
            );
        }
        if room.ceiling {
            push(
                PartKind::Ceiling,
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
                PartKind::Step,
                stair.base + dir * (STEP_DEPTH * (i as f32 + 0.5)) + Vec3::Y * (height / 2.0),
                dir.abs() * STEP_DEPTH + across * stair.width + Vec3::Y * height,
                def.floor_color,
            );
        }
        if stair.landing > 0.0 {
            push(
                PartKind::Landing,
                stair.base
                    + dir * (STEP_DEPTH * steps as f32 + stair.landing / 2.0)
                    + Vec3::Y * (stair.rise - SLAB / 2.0),
                dir.abs() * stair.landing + across * stair.width + Vec3::Y * SLAB,
                def.floor_color,
            );
        }
    }
    for part in &def.furniture {
        push(PartKind::Furniture, part.center, part.size, part.color);
    }
    parts
}

/// The part containing a structure-local point, if any: the hit to
/// part lookup. A point on a shared face belongs to the first part
/// in list order.
pub fn part_at(parts: &[Part], point: Vec3) -> Option<usize> {
    const SKIN: f32 = 1e-3;
    parts.iter().position(|p| {
        let a = p.aabb();
        point.x >= a.min.x - SKIN
            && point.x <= a.max.x + SKIN
            && point.y >= a.min.y - SKIN
            && point.y <= a.max.y + SKIN
            && point.z >= a.min.z - SKIN
            && point.z <= a.max.z + SKIN
    })
}

/// Land `hit` on part `index` through the one damage function
/// (a part is a target with health and no armor). A part at zero
/// health is removed; `removed` tells the consumer to rebake.
pub fn damage(parts: &mut Vec<Part>, index: usize, hit: &Hit<'_>) -> Option<PartHit> {
    let part = parts.get_mut(index)?;
    let mut health = Health {
        current: part.health,
        max: PART_HEALTH,
    };
    let result = resolve_hit(hit, &mut Protection::default(), &mut health);
    part.health = health.current;
    let removed = result.killed;
    if removed {
        parts.remove(index);
    }
    Some(PartHit { result, removed })
}

/// What a hit did to a part.
#[derive(Clone, Debug, PartialEq)]
pub struct PartHit {
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

/// What a shell slot is for. A builder supplies one part per
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

/// One part of a room's shell that a builder must supply: where
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
    /// Span the part must cover: width along the slot's run, and
    /// height (walls) or depth (floors and ceilings).
    pub width: f32,
    pub height: f32,
    pub opening: Option<SlotOpening>,
}

/// Greedy fill of a run using the largest modules that fit, so a
/// 7 m wall becomes 4 + 2 + 1 when the kit has those sizes.
/// `modules` must be sorted largest first. Returns each part's
/// start offset along the run and its width.
pub fn fill_run(run: f32, modules: &[f32]) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let mut at = 0.0f32;
    // A hair of tolerance so float remainders do not spawn a
    // sliver part at the end of every wall.
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

/// How a modular kit names its parts.
///
/// Kits are named the same way almost everywhere: a prefix and
/// the part's size, with separate prefixes for the wall segments
/// that carry a doorway or a window. What differs per game is the
/// prefixes, the units, and a short list of parts whose names
/// break the pattern.
///
/// Sizes in the name are in the kit's own units, usually
/// centimetres, so `units_per_metre` converts.
///
/// `openings` is BOTH the exception list and the availability
/// list: a doorway or window is only used at a size listed there.
/// Anything else falls back to a solid wall, so a room is never
/// left with a hole it has no part for.
///
/// `walls` overrides plain wall names for parts whose name lies
/// about their size.
#[derive(Clone, Copy)]
pub struct KitNames {
    pub wall: &'static str,
    pub floor: &'static str,
    pub units_per_metre: f32,
    /// `(opening, width, height, exact name)`.
    pub openings: &'static [(SlotOpening, i32, i32, &'static str)],
    /// `(width, height, exact name)`.
    pub walls: &'static [(i32, i32, &'static str)],
}

impl KitNames {
    /// A metre span in the kit's naming units.
    pub fn units(&self, metres: f32) -> i32 {
        (metres * self.units_per_metre).round() as i32
    }

    /// The plain wall part at a size.
    pub fn wall_at(&self, w: i32, h: i32) -> String {
        match self.walls.iter().find(|(a, b, _)| *a == w && *b == h) {
            Some((_, _, name)) => (*name).to_string(),
            None => format!("{}_{w}x{h}", self.wall),
        }
    }

    /// The floor or ceiling tile at a size.
    ///
    /// Sorted, because a 2 by 4 tile and a 4 by 2 tile are the
    /// same part turned round, and a kit only ships one of them.
    pub fn floor_at(&self, w: i32, d: i32) -> String {
        let (a, b) = if w <= d { (w, d) } else { (d, w) };
        format!("{}_{a}x{b}", self.floor)
    }

    /// The part that fills one slot.
    pub fn mesh_for(&self, slot: &ShellSlot) -> String {
        let w = self.units(slot.width);
        let h = self.units(slot.height);
        match slot.kind {
            SlotKind::Floor | SlotKind::Ceiling => self.floor_at(w, h),
            SlotKind::Wall => match slot.opening {
                None => self.wall_at(w, h),
                Some(op) => self
                    .openings
                    .iter()
                    .find(|(o, a, b, _)| *o == op && *a == w && *b == h)
                    .map(|(_, _, _, name)| (*name).to_string())
                    // No part at that size: a solid wall beats a
                    // hole.
                    .unwrap_or_else(|| self.wall_at(w, h)),
            },
        }
    }
}

/// Build the parts for one room, given a way to name the mesh
/// for each slot.
///
/// The shell (which tiles, which wall segments, which one carries
/// the door, and how a 7 m wall breaks into 4 + 2 + 1) is worked
/// out here. `mesh_for` supplies the one thing only the host game
/// knows: what its kit calls the part that fills a slot.
///
/// `modules` are the kit's available widths, largest first.
///
/// Positions and facings come straight from the slots, so the
/// result is in this crate's space: metres, y up, radians.
pub fn room_parts(
    room: &RoomDef,
    modules: &[f32],
    class: &str,
    mesh_for: impl Fn(&ShellSlot) -> String,
) -> Vec<PartDef> {
    shell_slots(room, modules)
        .into_iter()
        .map(|slot| PartDef {
            class: class.to_string(),
            asset: Some(mesh_for(&slot)),
            offset: slot.position,
            yaw: slot.yaw,
            pitch: 0.0,
            roll: 0.0,
            scale: 1.0,
            // The builder needs no measurements; the mesh carries
            // its own geometry.
            extent: Vec3::ZERO,
        })
        .collect()
}

/// A room described over the wire: interior size in metres, wall
/// height, and whether it gets a door and windows.
///
/// A door on the south wall and a window on each other side, both
/// on by default. Missing fields take the defaults, so a caller
/// can ask for a room with no arguments at all.
///
/// Sizes are the INTERIOR: a caller asking for 8 by 8 gets 8 by 8
/// of floor, with the walls outside that.
pub fn room_from_json(args: &serde_json::Value) -> RoomDef {
    let f = |k: &str, d: f32| args.get(k).and_then(|v| v.as_f64()).unwrap_or(d as f64) as f32;
    let b = |k: &str, d: bool| args.get(k).and_then(|v| v.as_bool()).unwrap_or(d);

    let mut openings = Vec::new();
    if b("door", true) {
        openings.push(Opening {
            side: Side::South,
            offset: 0.0,
            width: f("door_width", 1.2),
            sill: 0.0,
            door: true,
        });
    }
    if b("windows", true) {
        for side in [Side::North, Side::East, Side::West] {
            openings.push(Opening {
                side,
                offset: 0.0,
                width: f("window_width", 1.0),
                sill: f("window_sill", 1.0),
                door: false,
            });
        }
    }
    RoomDef {
        origin: Vec3::ZERO,
        interior: Vec3::new(f("width", 8.0), f("height", 3.0), f("length", 8.0)),
        wall_thickness: f("wall_thickness", 0.2),
        openings,
        floor: b("floor", true),
        // Off by default: a ceiling hides what you built.
        ceiling: b("ceiling", false),
    }
}

/// Decompose a room into the shell parts a builder must supply:
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
            // never match; the part that contains the doorway is
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
        assert_eq!(classify(e(4.0, 0.1, 4.0)), PartShape::Slab);
    }

    #[test]
    fn wall_panel_is_a_panel() {
        // 4m long, 20cm thick, 3m tall.
        assert_eq!(classify(e(2.0, 1.5, 0.1)), PartShape::Panel);
        // Same wall turned the other way.
        assert_eq!(classify(e(0.1, 1.5, 2.0)), PartShape::Panel);
    }

    #[test]
    fn pillar_is_a_post() {
        // 40cm square, 3m tall.
        assert_eq!(classify(e(0.2, 1.5, 0.2)), PartShape::Post);
    }

    #[test]
    fn pipe_is_a_beam() {
        // 6m long, 30cm through.
        assert_eq!(classify(e(3.0, 0.15, 0.15)), PartShape::Beam);
    }

    #[test]
    fn crate_is_a_block() {
        assert_eq!(classify(e(0.6, 0.6, 0.6)), PartShape::Block);
    }

    #[test]
    fn small_prop_is_clutter() {
        assert_eq!(classify(e(0.1, 0.2, 0.15)), PartShape::Clutter);
    }

    #[test]
    fn unmeasured_is_unknown() {
        assert_eq!(classify(Vec3::ZERO), PartShape::Unknown);
    }

    #[test]
    fn scale_is_applied_before_classifying() {
        // A clutter-sized box scaled up ten times is architecture.
        let p = PartDef {
            class: "x".into(),
            asset: None,
            offset: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            scale: 10.0,
            extent: e(0.2, 0.15, 0.02),
        };
        assert_eq!(p.shape(), PartShape::Panel);
    }

    const TEST_KIT: KitNames = KitNames {
        wall: "SM_Wall",
        floor: "SM_Floor",
        units_per_metre: 100.0,
        openings: &[
            (SlotOpening::Door, 400, 300, "SM_WallDoor_400x300"),
            (SlotOpening::Window, 400, 300, "SM_WallWindow_400x300"),
        ],
        walls: &[(400, 400, "SM_Wall_400x401")],
    };

    fn slot(kind: SlotKind, w: f32, h: f32, opening: Option<SlotOpening>) -> ShellSlot {
        ShellSlot {
            kind,
            position: Vec3::ZERO,
            yaw: 0.0,
            width: w,
            height: h,
            opening,
        }
    }

    #[test]
    fn a_part_is_named_by_its_size_in_the_kits_units() {
        assert_eq!(TEST_KIT.wall_at(200, 300), "SM_Wall_200x300");
        assert_eq!(TEST_KIT.units(4.0), 400);
    }

    /// Some parts lie about their size in their name.
    #[test]
    fn a_listed_wall_uses_its_exact_name() {
        assert_eq!(TEST_KIT.wall_at(400, 400), "SM_Wall_400x401");
    }

    /// A 2 by 4 tile and a 4 by 2 tile are one part turned round.
    #[test]
    fn a_floor_tile_is_named_the_same_either_way_round() {
        assert_eq!(TEST_KIT.floor_at(200, 400), TEST_KIT.floor_at(400, 200));
        assert_eq!(TEST_KIT.floor_at(400, 200), "SM_Floor_200x400");
    }

    #[test]
    fn a_slot_with_an_opening_gets_the_part_for_it() {
        let s = slot(SlotKind::Wall, 4.0, 3.0, Some(SlotOpening::Door));
        assert_eq!(TEST_KIT.mesh_for(&s), "SM_WallDoor_400x300");
        let s = slot(SlotKind::Wall, 4.0, 3.0, Some(SlotOpening::Window));
        assert_eq!(TEST_KIT.mesh_for(&s), "SM_WallWindow_400x300");
    }

    /// The kit has no doorway at this size, so a solid wall goes
    /// in. A hole would be worse than a missing door.
    #[test]
    fn an_unavailable_opening_falls_back_to_a_solid_wall() {
        let s = slot(SlotKind::Wall, 1.0, 3.0, Some(SlotOpening::Door));
        assert_eq!(TEST_KIT.mesh_for(&s), "SM_Wall_100x300");
    }

    #[test]
    fn floors_and_ceilings_use_the_floor_part() {
        for kind in [SlotKind::Floor, SlotKind::Ceiling] {
            let s = slot(kind, 4.0, 2.0, None);
            assert_eq!(TEST_KIT.mesh_for(&s), "SM_Floor_200x400");
        }
    }

    #[test]
    fn a_room_from_nothing_takes_the_defaults() {
        let r = room_from_json(&serde_json::json!({}));
        assert_eq!(r.interior, Vec3::new(8.0, 3.0, 8.0));
        assert!(r.floor);
        assert!(!r.ceiling, "a ceiling would hide what was built");
        // A door south, a window on each of the other three.
        assert_eq!(r.openings.len(), 4);
        assert_eq!(r.openings.iter().filter(|o| o.door).count(), 1);
    }

    #[test]
    fn room_json_reads_the_interior_not_the_outside() {
        let r = room_from_json(&serde_json::json!({"width": 5.0, "length": 12.0, "height": 4.0}));
        assert_eq!(r.interior, Vec3::new(5.0, 4.0, 12.0));
    }

    #[test]
    fn openings_can_be_turned_off() {
        let r = room_from_json(&serde_json::json!({"door": false, "windows": false}));
        assert!(r.openings.is_empty());
    }

    fn at(x: f32, y: f32, z: f32) -> PartDef {
        PartDef {
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
    fn nearby_parts_group_and_distant_ones_do_not() {
        let mut parts = Vec::new();
        for i in 0..4 {
            parts.push(at(i as f32 * 0.5, 0.0, 0.0));
        }
        for i in 0..4 {
            parts.push(at(10.0 + i as f32 * 0.5, 0.0, 0.0));
        }
        let groups = group_nearby(&parts, 3.0, 2, 100);
        assert_eq!(groups.len(), 2, "got {} group(s)", groups.len());
        assert_eq!(groups[0].len(), 4);
        assert_eq!(groups[1].len(), 4);
    }

    #[test]
    fn a_group_below_the_minimum_is_dropped() {
        let parts: Vec<PartDef> = (0..3).map(|i| at(i as f32 * 0.1, 0.0, 0.0)).collect();
        assert!(group_nearby(&parts, 1.0, 5, 100).is_empty());
    }

    /// Over the maximum, only the seed is dropped and the rest
    /// regroup. A dense area yields smaller things rather than
    /// nothing at all.
    #[test]
    fn an_oversized_group_regroups_smaller() {
        let parts: Vec<PartDef> = (0..3).map(|i| at(i as f32 * 0.1, 0.0, 0.0)).collect();
        let groups = group_nearby(&parts, 1.0, 1, 2);
        assert!(!groups.is_empty(), "a dense area should still yield something");
        assert!(
            groups.iter().all(|g| g.len() <= 2),
            "every group must respect the maximum"
        );
    }

    /// Height must not split a group, or an upper floor becomes
    /// its own building.
    #[test]
    fn height_does_not_separate_parts() {
        let parts = vec![at(0.0, 0.0, 0.0), at(0.5, 40.0, 0.0)];
        let groups = group_nearby(&parts, 3.0, 2, 100);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    /// A group comes back centred on itself, sitting on y = 0, so
    /// it can be put down anywhere.
    #[test]
    fn a_group_is_recentred_on_its_own_middle() {
        let parts = vec![at(100.0, 5.0, 100.0), at(102.0, 7.0, 100.0)];
        let groups = group_nearby(&parts, 5.0, 2, 100);
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
    fn no_part_lands_in_two_groups() {
        let parts: Vec<PartDef> = (0..12).map(|i| at(i as f32 * 0.4, 0.0, 0.0)).collect();
        let groups = group_nearby(&parts, 1.0, 2, 100);
        let total: usize = groups.iter().map(|g| g.len()).sum();
        assert!(total <= parts.len(), "{total} placed from {}", parts.len());
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
            parts: Vec::new(),
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
        small.parts = vec![at(0.0, 0.0, 0.0)];
        let mut big = structure("big");
        big.parts = (0..9).map(|i| at(i as f32, 0.0, 0.0)).collect();
        let mut middling = structure("middling");
        middling.parts = (0..4).map(|i| at(i as f32, 0.0, 0.0)).collect();

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
        let parts: Vec<PartDef> = (0..4).map(|i| at(i as f32 * 0.5, 0.0, 0.0)).collect();
        let got = capture("a_square", &parts, 3.0, 2, 100, CONCRETE_WALL, CONCRETE_FLOOR);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "a_square");
        assert_eq!(got[0].wall_color, CONCRETE_WALL);
        assert_eq!(got[0].parts.len(), 4);
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
        let p = PartDef {
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
            parts: vec![],
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
    fn a_room_with_a_doorway_becomes_seven_parts_and_a_hit_finds_the_wall() {
        let mut r = room(Vec3::ZERO);
        r.openings.push(Opening {
            side: Side::North,
            offset: 0.0,
            width: 1.2,
            sill: 0.0,
            door: true,
        });
        let d = def(vec![r]);
        let mut parts = parts_of(&d);
        // Three whole walls, two segments beside the doorway, floor,
        // ceiling.
        assert_eq!(parts.len(), 7);
        assert_eq!(parts.iter().filter(|p| p.kind == PartKind::Wall).count(), 5);

        // A point in the east wall band (x 3.0 to 3.2).
        let hit = part_at(&parts, Vec3::new(3.1, 1.0, 0.0)).expect("the east wall");
        assert_eq!(parts[hit].kind, PartKind::Wall);
        // The doorway gap has no part.
        assert!(part_at(&parts, Vec3::new(0.0, 1.0, -4.1)).is_none());

        // Damage short of death keeps it; death removes it. The
        // part goes through the one damage function.
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
        let first = damage(&mut parts, hit, &swing).unwrap();
        assert!(!first.removed);
        assert_eq!(first.result.damage_dealt, 40.0);
        assert_eq!(parts.len(), 7);
        assert!(!damage(&mut parts, hit, &swing).unwrap().removed);
        assert!(damage(&mut parts, hit, &swing).unwrap().removed, "120 of 100 health");
        assert_eq!(parts.len(), 6);
        assert!(part_at(&parts, Vec3::new(3.1, 1.0, 0.0)).is_none());
        assert!(damage(&mut parts, 99, &swing).is_none());
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
