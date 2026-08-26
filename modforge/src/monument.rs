//! Monuments as topside design.md "Monuments" describes them: a
//! monument TYPE rolls which building types it includes from its
//! list, each building generates from parameters (room count,
//! floors, footprint, doors, windows, palette), the buildings sit
//! by an arrangement rule, loot spots, NPC spots, and gates come
//! with it, and even the name is random. Never the same twice.
//!
//! Like [`crate::item`] and [`crate::biome`]: modforge owns the
//! shapes ([`BuildingTypeDef`], [`MonumentTypeDef`]) and the
//! registries; the consumer registers which building and monument
//! types exist in its game. Engine-agnostic and deterministic: the
//! same seed rolls the same monument. The output is a
//! [`MonumentDef`] of plain [`StructureDef`]s; the consumer's
//! spawner places them.
//!
//! Prior art: Rust's monuments and its card rooms (the gates).

use glam::Vec3;

use crate::structure::{
    Aabb, Gate, LightSpec, LootSpot, MonumentDef, MonumentMember, NpcSpot, Opening, Rgb,
    RoomSpec, SLAB, STEP_DEPTH, STEP_RISE_MAX, Side, SolidSpec, StairSpec, StructureDef,
    room_interior_aabb, validate,
};
use crate::unknown::rng;

const WALL: f32 = 0.2;
const DOOR_WIDTH: f32 = 1.2;
const WINDOW_WIDTH: f32 = 1.0;
const WINDOW_SILL: f32 = 1.0;
/// Most floors a building goes up, and most it goes down.
pub const MAX_LEVELS: u32 = 10;
/// The stair tower: two flights per level side by side (a
/// switchback), each STAIR_WIDTH wide, with a half landing at the
/// far end and a landing slab at the near end where every level's
/// doorway sits.
const STAIRWELL_WIDTH: f32 = 2.0 * STAIR_WIDTH + 3.0 * WALL;
const STAIR_WIDTH: f32 = 1.8;
const HALF_LANDING: f32 = 1.8;
/// Flat room in front of the first step, and the slab each level's
/// doorway opens onto.
const STAIR_APPROACH: f32 = 1.0;

/// A seeded roll stream: every draw advances the salt, so one seed
/// yields one reproducible sequence. Built on the one random
/// function, [`rng`].
pub struct Roll {
    seed: u64,
    draws: u64,
}

impl Roll {
    pub fn new(seed: u64) -> Self {
        Self { seed, draws: 0 }
    }

    /// A value in [0, n).
    pub fn next(&mut self, n: u64) -> u64 {
        self.draws += 1;
        let now = f32::from_bits((self.seed as u32) | 0x3F80_0000);
        rng(now, self.draws ^ (self.seed >> 32).wrapping_mul(0x9E37_79B9), n)
    }

    /// An integer in [lo, hi].
    pub fn between(&mut self, lo: u32, hi: u32) -> u32 {
        lo + self.next((hi.max(lo) - lo) as u64 + 1) as u32
    }

    /// A float in [lo, hi), in steps of 0.1.
    pub fn measure(&mut self, lo: f32, hi: f32) -> f32 {
        let steps = ((hi - lo) * 10.0).max(1.0) as u64;
        lo + self.next(steps) as f32 / 10.0
    }

    pub fn chance(&mut self, per_mille: u64) -> bool {
        self.next(1000) < per_mille
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.next(items.len() as u64) as usize]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuildingSize {
    Small,
    Medium,
    Large,
}

/// One building type as data (design.md "Building types"): what
/// varies on every roll (operator-locked: "the buildings should
/// have parameters that change"). Rooms per floor fill a grid of
/// `columns` by `rows`; floors stack. `name` is the id.
#[derive(Clone, Debug)]
pub struct BuildingTypeDef {
    pub name: String,
    pub size: BuildingSize,
    /// Rooms across (east-west) per floor.
    pub columns: (u32, u32),
    /// Rooms deep (north-south) per floor.
    pub rows: (u32, u32),
    /// Floors from the ground up, at least 1, at most MAX_LEVELS.
    pub floors: (u32, u32),
    /// Floors below ground (a bunker goes down), at most MAX_LEVELS.
    pub basements: (u32, u32),
    /// Interior width of one column of rooms.
    pub width: (f32, f32),
    /// Interior length of one row of rooms.
    pub length: (f32, f32),
    /// Floor height, rolled once per building.
    pub height: (f32, f32),
    /// Chance per exterior wall of a window, per mille.
    pub windows: u64,
    /// Chance per room of a piece of furniture, per mille.
    pub clutter: u64,
    /// Chance per room of a light, per mille. The first room of a
    /// building is always lit.
    pub lights: u64,
    /// Condition (design.md): chance per exterior wall of a breach
    /// and per top room of a missing roof, per mille. 0 is intact.
    pub damage: u64,
    /// Chance per far-corner room of being carved away, per mille,
    /// so footprints come out as L and step shapes, not boxes.
    pub carve: u64,
    /// Wall and floor colours are drawn from here per building;
    /// furniture per piece.
    pub palette: Vec<Rgb>,
}

impl BuildingTypeDef {
    /// Fewest and most rooms per floor this type can roll.
    pub fn rooms_per_floor(&self) -> (u32, u32) {
        (self.columns.0 * self.rows.0, self.columns.1 * self.rows.1)
    }
}

/// The checked-in building types. The consumer registers its
/// content at startup and looks types up by name.
#[derive(Default)]
pub struct BuildingRegistry {
    defs: Vec<BuildingTypeDef>,
}

impl BuildingRegistry {
    pub fn register(&mut self, def: BuildingTypeDef) -> Result<(), String> {
        if self.defs.iter().any(|d| d.name == def.name) {
            return Err(format!("building type '{}' registered twice", def.name));
        }
        if def.palette.is_empty() {
            return Err(format!("building type '{}' has an empty palette", def.name));
        }
        if def.floors.0 < 1 || def.floors.1 > MAX_LEVELS || def.basements.1 > MAX_LEVELS {
            return Err(format!(
                "building type '{}': 1 to {MAX_LEVELS} floors up and 0 to {MAX_LEVELS} down",
                def.name
            ));
        }
        self.defs.push(def);
        Ok(())
    }

    pub fn def(&self, name: &str) -> Option<&BuildingTypeDef> {
        self.defs.iter().find(|d| d.name == name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.defs.iter().map(|d| d.name.as_str())
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

/// Roll one building of type `def`: a grid of rooms per floor
/// (columns east-west, rows running north), doorways between every
/// pair of neighbours, one door out the south of the front room,
/// windows on exterior walls by chance, a full-height stairwell
/// east of the front row when there is a second floor, furniture
/// and lights by chance. Every result passes [`validate`].
pub fn generate_building(def: &BuildingTypeDef, roll: &mut Roll) -> StructureDef {
    let p = def;
    let columns = roll.between(p.columns.0, p.columns.1) as usize;
    let rows = roll.between(p.rows.0, p.rows.1) as usize;
    let floors = roll.between(p.floors.0, p.floors.1) as i32;
    let basements = roll.between(p.basements.0, p.basements.1) as i32;
    let levels = floors + basements;
    let height = roll.measure(p.height.0, p.height.1);
    let widths: Vec<f32> = (0..columns)
        .map(|_| roll.measure(p.width.0, p.width.1))
        .collect();
    let lengths: Vec<f32> = (0..rows)
        .map(|_| roll.measure(p.length.0, p.length.1))
        .collect();
    let wall_color = *roll.pick(&p.palette);
    let floor_color = {
        let c = *roll.pick(&p.palette);
        [c[0] * 0.7, c[1] * 0.7, c[2] * 0.7]
    };

    // Room origins: neighbours are 2 wall thicknesses apart (each
    // room builds its own wall on the shared plane). Columns run
    // east (+x), rows run north (-z) from the front row at z 0.
    let xs = spaced(&widths, 1.0);
    let zs = spaced(&lengths, -1.0);
    let last_col = columns - 1;
    let last_row = rows - 1;
    let stairwell_x = xs[last_col] + widths[last_col] / 2.0 + 2.0 * WALL + STAIRWELL_WIDTH / 2.0;

    // Carve rooms off the far corner inward so the footprint is an
    // L or a step shape, never a bare box. A room goes only when
    // everything east of it and north of it is already gone, so
    // what remains still reaches the front door along the grid; the
    // front room and the stairwell's room never go.
    let mut present = vec![vec![true; columns]; rows];
    for r in (0..rows).rev() {
        for c in (0..columns).rev() {
            let keep = (c == 0 && r == 0) || (c == last_col && r == 0 && levels > 1);
            let corner = (c == last_col || !present[r][c + 1]) && (r == last_row || !present[r + 1][c]);
            if !keep && corner && roll.chance(p.carve) {
                present[r][c] = false;
            }
        }
    }
    let has = |r: isize, c: isize| -> bool {
        r >= 0 && c >= 0 && (r as usize) < rows && (c as usize) < columns && present[r as usize][c as usize]
    };

    // Shared doorway offsets, rolled once per wall so both rooms
    // author the same gap: east walls keyed by (r, c), north walls
    // keyed by (r, c).
    let slack = |run: f32| (run / 2.0 - DOOR_WIDTH / 2.0 - 0.4).max(0.0);
    let east_offsets: Vec<Vec<f32>> = (0..rows)
        .map(|r| (0..columns).map(|_| roll.measure(-slack(lengths[r]), slack(lengths[r]))).collect())
        .collect();
    let north_offsets: Vec<Vec<f32>> = (0..rows)
        .map(|_| (0..columns).map(|c| roll.measure(-slack(widths[c]), slack(widths[c]))).collect())
        .collect();

    let mut rooms = Vec::new();
    let mut furniture = Vec::new();
    let mut lights = Vec::new();
    // Levels run from the deepest basement (negative) to the top
    // floor; 0 is the ground. Basements are below ground but not
    // underground: their rooms sit beneath the ground rooms, and
    // the consumer's terrain is expected to be flat at y 0.
    for floor in -basements..floors {
        let y = floor as f32 * height;
        let top = floor + 1 == floors;
        let exterior_level = floor >= 0;
        for (r, (&z, &l)) in zs.iter().zip(&lengths).enumerate() {
            for (c, (&x, &w)) in xs.iter().zip(&widths).enumerate() {
                if !present[r][c] {
                    continue;
                }
                let (ri, ci) = (r as isize, c as isize);
                let front = c == 0 && r == 0;
                let mut openings = Vec::new();
                // The door out: south of the front room, ground floor.
                if front && floor == 0 {
                    openings.push(door(Side::South, 0.0));
                }
                // Doorways to neighbours: the west and south rooms own
                // the hinge, the east and north rooms author the gap.
                let west = has(ri, ci - 1);
                let east = has(ri, ci + 1);
                let south = has(ri - 1, ci);
                let north = has(ri + 1, ci);
                if west {
                    openings.push(Opening {
                        door: false,
                        ..door(Side::West, east_offsets[r][c - 1])
                    });
                }
                if east {
                    openings.push(door(Side::East, east_offsets[r][c]));
                }
                if south {
                    openings.push(Opening {
                        door: false,
                        ..door(Side::South, north_offsets[r - 1][c])
                    });
                }
                if north {
                    openings.push(door(Side::North, north_offsets[r][c]));
                }
                // Into the stair tower, east of the front row, at
                // every level.
                let stair_door = c == last_col && r == 0 && levels > 1;
                if stair_door {
                    openings.push(Opening {
                        door: false,
                        ..door(Side::East, 0.0)
                    });
                }
                // Exterior walls above ground: windows by chance,
                // breaches by damage. A window sits off-centre; a
                // breach is a ragged gap down near the floor.
                let exterior = [
                    (Side::North, !north, w),
                    (Side::South, !south && !(front && floor == 0), w),
                    (Side::West, !west, l),
                    (Side::East, !east && !stair_door, l),
                ];
                for (side, free, run) in exterior {
                    if !free || !exterior_level {
                        continue;
                    }
                    if run >= 2.0 * WINDOW_WIDTH + 1.0 && roll.chance(p.windows) {
                        let s = (run / 2.0 - WINDOW_WIDTH / 2.0 - 0.4).max(0.0);
                        openings.push(window(side, roll.measure(-s, s)));
                    } else if run >= 4.0 && roll.chance(p.damage) {
                        let width = roll.measure(1.5, (run - 1.0).min(3.5));
                        let s = (run / 2.0 - width / 2.0 - 0.3).max(0.0);
                        openings.push(Opening {
                            side,
                            offset: roll.measure(-s, s),
                            width,
                            sill: roll.measure(0.0, 0.6),
                            door: false,
                        });
                    }
                }
                let origin = Vec3::new(x, y, z);
                rooms.push(RoomSpec {
                    origin,
                    interior: Vec3::new(w, height, l),
                    wall_thickness: WALL,
                    openings,
                    floor: true,
                    // Only the top floor roofs itself, and roofs go
                    // missing with damage.
                    ceiling: top && !roll.chance(p.damage),
                });
                let pieces = roll.between(0, 2) as usize + usize::from(roll.chance(p.clutter));
                for _ in 0..pieces.min(3) {
                    let size = Vec3::new(
                        roll.measure(0.5, 1.6),
                        roll.measure(0.3, 1.2),
                        roll.measure(0.5, 1.4),
                    );
                    // Anywhere a doorway is not: the doorways are at
                    // the wall centres, furniture keeps to the corners.
                    let sx = (w / 2.0 - size.x / 2.0 - 0.2).max(0.0);
                    let sz = (l / 2.0 - size.z / 2.0 - 0.2).max(0.0);
                    let dx = sx * if roll.chance(500) { 1.0 } else { -1.0 } * roll.measure(0.6, 1.0);
                    let dz = sz * if roll.chance(500) { 1.0 } else { -1.0 } * roll.measure(0.6, 1.0);
                    furniture.push(SolidSpec {
                        center: origin + Vec3::new(dx, size.y / 2.0, dz),
                        size,
                        color: *roll.pick(&p.palette),
                    });
                }
                if (front && floor == 0) || roll.chance(p.lights) {
                    lights.push(LightSpec {
                        position: origin + Vec3::new(0.0, height - 0.4, 0.0),
                        color: [1.0, 0.9, 0.7],
                        intensity: 400_000.0,
                    });
                }
            }
        }
    }

    // The stair tower: one room spanning every level east of the
    // front row. Each level climbs by a switchback: the up flight
    // in the east lane from the south landing slab to a half
    // landing at the north end, the return flight in the west lane
    // back to the next level's landing slab at the south end. Every
    // level's doorway opens onto its landing slab at z 0.
    let mut stairs = Vec::new();
    if levels > 1 {
        let half_steps = ((height / 2.0) / STEP_RISE_MAX).ceil().max(1.0);
        let run = half_steps * STEP_DEPTH;
        // South to north: approach, slab zone, up flight, half landing.
        let slab_depth = STAIR_APPROACH + HALF_LANDING;
        let length = slab_depth + run + HALF_LANDING + WALL;
        // The tower's z origin puts the slab zone's centre at z 0.
        let south_wall = length / 2.0;
        let slab_centre_local = south_wall - slab_depth / 2.0;
        let tower_z = -slab_centre_local;
        let bottom = -(basements as f32) * height;
        let total = levels as f32 * height;
        let east_lane = stairwell_x + STAIR_WIDTH / 2.0 + WALL / 2.0;
        let west_lane = stairwell_x - STAIR_WIDTH / 2.0 - WALL / 2.0;

        let mut openings = Vec::new();
        for level in -basements..floors {
            openings.push(Opening {
                side: Side::West,
                offset: 0.0,
                width: DOOR_WIDTH,
                sill: (level as f32 * height - bottom) + if level > -basements { SLAB } else { 0.0 },
                door: false,
            });
        }
        rooms.push(RoomSpec {
            origin: Vec3::new(stairwell_x, bottom, tower_z),
            interior: Vec3::new(STAIRWELL_WIDTH, total, length),
            wall_thickness: WALL,
            openings,
            floor: true,
            ceiling: true,
        });
        for level in -basements..floors {
            let y = level as f32 * height;
            // The landing slab every level stands on at its doorway
            // (the bottom level has the tower's own floor).
            if level > -basements {
                furniture.push(SolidSpec {
                    center: Vec3::new(stairwell_x, y - SLAB / 2.0, tower_z + slab_centre_local),
                    size: Vec3::new(STAIRWELL_WIDTH, SLAB, slab_depth),
                    color: floor_color,
                });
            }
            if level + 1 < floors {
                // Up flight: east lane, from the slab's north edge to
                // the half landing.
                let up_base_z = tower_z + south_wall - slab_depth;
                stairs.push(StairSpec {
                    base: Vec3::new(east_lane, y, up_base_z),
                    side: Side::North,
                    width: STAIR_WIDTH,
                    rise: height / 2.0,
                    landing: 0.1,
                });
                // The half landing spans both lanes so the turn is
                // walkable.
                let half_top = up_base_z - run;
                furniture.push(SolidSpec {
                    center: Vec3::new(
                        stairwell_x,
                        y + height / 2.0 - SLAB / 2.0,
                        half_top - HALF_LANDING / 2.0,
                    ),
                    size: Vec3::new(STAIRWELL_WIDTH, SLAB, HALF_LANDING),
                    color: floor_color,
                });
                // Return flight: west lane, from the half landing's
                // south edge back south to the next level's slab.
                stairs.push(StairSpec {
                    base: Vec3::new(west_lane, y + height / 2.0, half_top),
                    side: Side::South,
                    width: STAIR_WIDTH,
                    rise: height / 2.0,
                    landing: 0.1,
                });
            }
            lights.push(LightSpec {
                position: Vec3::new(stairwell_x, y + height - 0.4, tower_z),
                color: [1.0, 0.9, 0.7],
                intensity: 300_000.0,
            });
        }
    }

    let out = StructureDef {
        name: def.name.clone(),
        wall_color,
        floor_color,
        rooms,
        stairs,
        furniture,
        lights,
        pieces: Vec::new(),
    };
    if let Err(e) = validate(&out) {
        debug_assert!(false, "generated buildings are legal: {e}");
    }
    out
}

/// Centre coordinates of rooms laid side by side with the given
/// interior extents, two wall thicknesses apart, starting at 0 and
/// advancing in `direction` (+1 east, -1 north).
fn spaced(extents: &[f32], direction: f32) -> Vec<f32> {
    let mut out = Vec::with_capacity(extents.len());
    let mut cursor = 0.0;
    for (i, e) in extents.iter().enumerate() {
        if i > 0 {
            cursor += direction * (extents[i - 1] / 2.0 + 2.0 * WALL + e / 2.0);
        }
        out.push(cursor);
    }
    out
}

fn door(side: Side, offset: f32) -> Opening {
    Opening {
        side,
        offset,
        width: DOOR_WIDTH,
        sill: 0.0,
        door: true,
    }
}

fn window(side: Side, offset: f32) -> Opening {
    Opening {
        side,
        offset,
        width: WINDOW_WIDTH,
        sill: WINDOW_SILL,
        door: false,
    }
}

/// The ground footprint of a structure: every room interior plus
/// its walls, and every captured piece, flattened to the ground.
/// A captured structure has no rooms to measure, so its pieces
/// carry the extent instead.
pub fn footprint(def: &StructureDef) -> Aabb {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for room in &def.rooms {
        let a = room_interior_aabb(room);
        let t = Vec3::new(room.wall_thickness, 0.0, room.wall_thickness);
        min = min.min(a.min - t);
        max = max.max(a.max + t);
    }
    for piece in &def.pieces {
        min = min.min(piece.offset);
        max = max.max(piece.offset);
    }
    if def.rooms.is_empty() && def.pieces.is_empty() {
        return Aabb {
            min: Vec3::ZERO,
            max: Vec3::ZERO,
        };
    }
    Aabb { min, max }
}

fn shifted(a: &Aabb, by: Vec3) -> Aabb {
    Aabb {
        min: a.min + by,
        max: a.max + by,
    }
}

/// How a monument's buildings sit relative to each other (design.md
/// "Arrangement rules").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arrangement {
    Clustered,
    AlongRoad,
    AroundYard,
}

/// One pick from a monument type's building list: `min` to `max`
/// buildings drawn from `choices` (building type names).
#[derive(Clone, Debug)]
pub struct BuildingSlot {
    pub choices: Vec<String>,
    pub min: u32,
    pub max: u32,
}

/// A prop a monument type scatters around its origin on every roll:
/// size, colour, how many of them, and how far out they stand.
#[derive(Clone, Debug, PartialEq)]
pub struct PropSpec {
    pub size: Vec3,
    pub color: Rgb,
    pub count: (u32, u32),
    pub radius: f32,
}

/// One monument type as data (design.md "Monument types"). `name`
/// is the id; `suffix` ends every rolled monument's name. A minor
/// site (design.md: "something worth stopping for every 30 seconds")
/// has no building slots, only props and a loot spot, and a small
/// `spacing`.
#[derive(Clone, Debug)]
pub struct MonumentTypeDef {
    pub name: String,
    pub slots: Vec<BuildingSlot>,
    pub arrangement: Arrangement,
    /// Base danger of its loot and NPC spots (design.md "Danger").
    pub danger: u32,
    /// Whether its last building hides a gated back room.
    pub gated: bool,
    pub suffix: String,
    /// Least distance from another site of any type when worldgen
    /// places it; and how much ground is pressed flat under it.
    pub spacing: f32,
    pub props: Vec<PropSpec>,
    /// What the site is good for to a person who knows it (life.md):
    /// a camp for rest and safety. Its boxes are good for food on
    /// their own.
    pub good_for: crate::memory::GoodFor,
    /// The tallest thing that stands there, in metres above its
    /// ground: what can be seen from far away (topside design.md "The
    /// triangle rule"). The tallest type in a world def is its
    /// landmark.
    pub height: f32,
}

/// The checked-in monument types plus the word pools rolled names
/// are built from. Registering a type checks that every building it
/// names exists in the building registry.
#[derive(Default)]
pub struct MonumentRegistry {
    defs: Vec<MonumentTypeDef>,
    name_first: Vec<String>,
    name_second: Vec<String>,
}

impl MonumentRegistry {
    pub fn register(
        &mut self,
        def: MonumentTypeDef,
        buildings: &BuildingRegistry,
    ) -> Result<(), String> {
        if self.defs.iter().any(|d| d.name == def.name) {
            return Err(format!("monument type '{}' registered twice", def.name));
        }
        if def.slots.is_empty() && def.props.is_empty() {
            return Err(format!(
                "monument type '{}' has no building slots and no props",
                def.name
            ));
        }
        if def.spacing <= 0.0 {
            return Err(format!("monument type '{}' needs a spacing", def.name));
        }
        for slot in &def.slots {
            if slot.choices.is_empty() || slot.min > slot.max {
                return Err(format!("monument type '{}': bad building slot", def.name));
            }
            for choice in &slot.choices {
                if buildings.def(choice).is_none() {
                    return Err(format!(
                        "monument type '{}' names unknown building type '{choice}'",
                        def.name
                    ));
                }
            }
        }
        self.defs.push(def);
        Ok(())
    }

    /// The two word pools a rolled name joins ("Ash" + "fall").
    pub fn set_name_words(&mut self, first: Vec<String>, second: Vec<String>) {
        self.name_first = first;
        self.name_second = second;
    }

    pub fn def(&self, name: &str) -> Option<&MonumentTypeDef> {
        self.defs.iter().find(|d| d.name == name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.defs.iter().map(|d| d.name.as_str())
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// A rolled monument name: two joined words and the type's
    /// suffix ("Ashfall Stop"). Random every time (design.md: "even
    /// the monument NAME is random"). Without word pools, the
    /// suffix alone.
    pub fn roll_name(&self, def: &MonumentTypeDef, roll: &mut Roll) -> String {
        if self.name_first.is_empty() || self.name_second.is_empty() {
            return def.suffix.clone();
        }
        format!(
            "{}{} {}",
            roll.pick(&self.name_first),
            roll.pick(&self.name_second),
            def.suffix
        )
    }

    /// Roll one monument of type `name` from `seed`: pick building
    /// types per the slots, generate each building, arrange them,
    /// then place loot spots (one per building, in its back room),
    /// NPC spots (one outside each front door), and the gate (the
    /// last building's back room, when gated).
    pub fn roll(
        &self,
        name: &str,
        buildings: &BuildingRegistry,
        seed: u64,
    ) -> Result<MonumentDef, String> {
        let def = self
            .def(name)
            .ok_or_else(|| format!("monument type '{name}' is not registered"))?;
        let mut roll = Roll::new(seed);
        let rolled_name = self.roll_name(def, &mut roll);

        let mut generated = Vec::new();
        for slot in &def.slots {
            let count = roll.between(slot.min, slot.max);
            for _ in 0..count {
                let kind = roll.pick(&slot.choices);
                let building = buildings
                    .def(kind)
                    .ok_or_else(|| format!("building type '{kind}' is not registered"))?;
                generated.push(generate_building(building, &mut roll));
            }
        }

        let members = arrange(generated, def.arrangement, &mut roll);

        let mut loot_spots = Vec::new();
        let mut npc_spots = Vec::new();
        let mut gates = Vec::new();
        let count = members.len();
        for (i, member) in members.iter().enumerate() {
            let first = &member.structure.rooms[0];
            let back = back_room(&member.structure);
            let gate_wall = gate_wall(back);
            let gated_here = def.gated && i + 1 == count && gate_wall.is_some();
            let danger = def.danger + u32::from(gated_here);
            loot_spots.push(LootSpot {
                position: member.offset + back.origin + Vec3::new(0.0, 0.35, 0.0),
                danger,
            });
            npc_spots.push(NpcSpot {
                position: member.offset
                    + first.origin
                    + Vec3::new(0.0, 0.0, first.interior.z / 2.0 + 2.0),
                danger: def.danger,
            });
            if let (true, Some(wall)) = (gated_here, gate_wall) {
                gates.push(Gate {
                    position: member.offset + wall,
                    level: def.danger,
                });
            }
        }

        // A site with no buildings keeps its loot at the origin.
        if members.is_empty() {
            loot_spots.push(LootSpot {
                position: Vec3::new(0.0, 0.35, 0.0),
                danger: def.danger,
            });
        }
        // Props in a ring around the origin, each at a rolled angle
        // and distance, never on the loot spot.
        let mut props = Vec::new();
        for spec in &def.props {
            let count = roll.between(spec.count.0, spec.count.1);
            for _ in 0..count {
                let angle = roll.measure(0.0, std::f32::consts::TAU);
                let distance = roll.measure(spec.radius * 0.4 + 1.0, spec.radius.max(1.5));
                props.push(crate::structure::Prop {
                    position: Vec3::new(
                        angle.cos() * distance,
                        spec.size.y / 2.0,
                        angle.sin() * distance,
                    ),
                    size: spec.size,
                    color: spec.color,
                });
            }
        }

        Ok(MonumentDef {
            name: rolled_name,
            kind: def.name.clone(),
            members,
            loot_spots,
            npc_spots,
            gates,
            props,
            good_for: def.good_for.clone(),
        })
    }
}

/// The back room of a building: the last ground-floor room of its
/// grid (the far corner from the front door), never the stairwell.
pub fn back_room(structure: &StructureDef) -> &RoomSpec {
    let height = structure.rooms[0].interior.y;
    structure
        .rooms
        .iter()
        .filter(|r| r.origin.y == 0.0 && r.interior.y <= height)
        .last()
        .unwrap_or(&structure.rooms[0])
}

/// Where the back room's gate goes: the centre of its west wall if
/// a neighbour's door is there, else its south wall's, else none
/// (a one-room building has no back room to gate).
fn gate_wall(back: &RoomSpec) -> Option<Vec3> {
    let t = back.wall_thickness;
    let doorway = |side| {
        back.openings
            .iter()
            .find(|o| o.side == side && o.sill == 0.0)
            .map(|o| o.offset)
    };
    if let Some(offset) = doorway(Side::West) {
        Some(back.origin + Vec3::new(-(back.interior.x / 2.0 + t), 0.0, offset))
    } else {
        doorway(Side::South)
            .map(|offset| back.origin + Vec3::new(offset, 0.0, back.interior.z / 2.0 + t))
    }
}

/// Place buildings by the rule on city-block cells sized to the
/// largest footprint plus a gap, so no two can overlap at any count.
/// Clustered: a compact grid of blocks around the origin. Around a
/// yard: the same grid with the middle cell left open. Along a
/// road: pairs of buildings face each other across streets that
/// run east, a new street every few pairs. Each building is jittered
/// inside the slack of its cell.
pub fn arrange(
    buildings: Vec<StructureDef>,
    arrangement: Arrangement,
    roll: &mut Roll,
) -> Vec<MonumentMember> {
    const GAP: f32 = 4.0;
    const ROAD: f32 = 8.0;
    let feet: Vec<Aabb> = buildings.iter().map(footprint).collect();
    let cell_x = feet.iter().map(|f| f.max.x - f.min.x).fold(0.0, f32::max) + GAP;
    let cell_z = feet.iter().map(|f| f.max.z - f.min.z).fold(0.0, f32::max) + GAP;
    let n = buildings.len();

    // Cell centres, one per building, by rule.
    let cells: Vec<Vec3> = match arrangement {
        Arrangement::Clustered | Arrangement::AroundYard => {
            let yard = arrangement == Arrangement::AroundYard;
            let slots = n + usize::from(yard);
            let columns = (slots as f32).sqrt().ceil().max(1.0) as usize;
            let rows = slots.div_ceil(columns);
            let middle = (rows / 2) * columns + columns / 2;
            (0..slots)
                .filter(|i| !(yard && *i == middle))
                .map(|i| {
                    let (c, r) = (i % columns, i / columns);
                    Vec3::new(
                        (c as f32 - (columns as f32 - 1.0) / 2.0) * cell_x,
                        0.0,
                        (r as f32 - (rows as f32 - 1.0) / 2.0) * cell_z,
                    )
                })
                .collect()
        }
        Arrangement::AlongRoad => {
            let per_street = ((n as f32 / 2.0).sqrt().ceil() as usize).clamp(1, 6);
            (0..n)
                .map(|i| {
                    let street = i / (2 * per_street);
                    let along = (i % (2 * per_street)) / 2;
                    let side = if i % 2 == 0 { -1.0 } else { 1.0 };
                    Vec3::new(
                        along as f32 * cell_x,
                        0.0,
                        street as f32 * (2.0 * cell_z + ROAD) + side * (ROAD + cell_z) / 2.0,
                    )
                })
                .collect()
        }
    };

    buildings
        .into_iter()
        .zip(feet)
        .zip(cells)
        .map(|((structure, foot), cell)| {
            let size = foot.max - foot.min;
            let centre = (foot.min + foot.max) / 2.0;
            let slack_x = (cell_x - GAP - size.x).max(0.0);
            let slack_z = (cell_z - GAP - size.z).max(0.0);
            let jitter = Vec3::new(
                roll.measure(-slack_x / 2.0, slack_x / 2.0),
                0.0,
                roll.measure(-slack_z / 2.0, slack_z / 2.0),
            );
            let offset = cell + jitter - centre.with_y(0.0);
            MonumentMember { structure, offset }
        })
        .collect()
}

/// The relation rules of a monument: every member is a legal
/// structure and no two footprints overlap.
pub fn validate_monument(def: &MonumentDef) -> Result<(), String> {
    let mut feet = Vec::new();
    for member in &def.members {
        validate(&member.structure).map_err(|e| format!("monument '{}': {e}", def.name))?;
        feet.push(shifted(&footprint(&member.structure), member.offset));
    }
    for (i, a) in feet.iter().enumerate() {
        for (j, b) in feet.iter().enumerate().skip(i + 1) {
            if a.overlaps(b) {
                return Err(format!(
                    "monument '{}': member {i} and member {j} footprints overlap",
                    def.name
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn building(
        name: &str,
        columns: (u32, u32),
        rows: (u32, u32),
        floors: (u32, u32),
        basements: (u32, u32),
    ) -> BuildingTypeDef {
        BuildingTypeDef {
            name: name.to_string(),
            size: BuildingSize::Small,
            columns,
            rows,
            floors,
            basements,
            width: (4.0, 8.0),
            length: (5.0, 9.0),
            height: (2.8, 3.4),
            windows: 500,
            clutter: 600,
            lights: 300,
            damage: 200,
            carve: 400,
            palette: vec![[0.5, 0.4, 0.3], [0.3, 0.3, 0.35]],
        }
    }

    fn registries() -> (BuildingRegistry, MonumentRegistry) {
        let mut b = BuildingRegistry::default();
        b.register(building("shack", (1, 1), (1, 1), (1, 1), (0, 0))).unwrap();
        b.register(building("office", (2, 4), (1, 2), (2, 2), (0, 0))).unwrap();
        b.register(building("warehouse", (2, 3), (1, 1), (1, 1), (0, 0))).unwrap();
        b.register(building("tower block", (4, 5), (4, 5), (2, 10), (0, 0))).unwrap();
        b.register(building("bunker", (2, 3), (2, 3), (1, 1), (3, 10))).unwrap();
        b.register(building("spire", (2, 2), (2, 2), (6, 10), (2, 10))).unwrap();
        let mut m = MonumentRegistry::default();
        m.set_name_words(
            vec!["Ash".into(), "Rust".into(), "Grey".into()],
            vec!["fall".into(), "ridge".into(), "well".into()],
        );
        m.register(
            MonumentTypeDef {
                name: "roadside stop".into(),
                slots: vec![BuildingSlot {
                    choices: vec!["shack".into(), "warehouse".into()],
                    min: 1,
                    max: 2,
                }],
                arrangement: Arrangement::AlongRoad,
                danger: 1,
                gated: false,
                suffix: "Stop".into(),
                spacing: 120.0,
                props: vec![],
                good_for: Default::default(),
                height: 6.0,
            },
            &b,
        )
        .unwrap();
        m.register(
            MonumentTypeDef {
                name: "launch site".into(),
                slots: vec![
                    BuildingSlot {
                        choices: vec!["office".into()],
                        min: 1,
                        max: 1,
                    },
                    BuildingSlot {
                        choices: vec!["warehouse".into(), "shack".into()],
                        min: 1,
                        max: 2,
                    },
                ],
                arrangement: Arrangement::AroundYard,
                danger: 5,
                gated: true,
                suffix: "Site".into(),
                spacing: 200.0,
                props: vec![],
                good_for: Default::default(),
                height: 6.0,
            },
            &b,
        )
        .unwrap();
        m.register(
            MonumentTypeDef {
                name: "city".into(),
                slots: vec![BuildingSlot {
                    choices: vec!["tower block".into(), "office".into()],
                    min: 50,
                    max: 50,
                }],
                arrangement: Arrangement::Clustered,
                danger: 3,
                gated: true,
                suffix: "City".into(),
                spacing: 300.0,
                props: vec![],
                good_for: Default::default(),
                height: 6.0,
            },
            &b,
        )
        .unwrap();
        (b, m)
    }

    #[test]
    fn registries_reject_unknown_buildings_and_duplicates() {
        let (mut b, mut m) = registries();
        assert!(b.register(building("shack", (1, 1), (1, 1), (1, 1), (0, 0))).is_err());
        assert!(
            b.register(building("too tall", (1, 1), (1, 1), (1, 11), (0, 0))).is_err(),
            "ten floors is the most"
        );
        assert!(b.def("shack").is_some() && b.def("tower").is_none());
        let bad = MonumentTypeDef {
            name: "bad".into(),
            slots: vec![BuildingSlot {
                choices: vec!["tower".into()],
                min: 1,
                max: 1,
            }],
            arrangement: Arrangement::Clustered,
            danger: 1,
            gated: false,
            suffix: "Bad".into(),
            spacing: 50.0,
            props: vec![],
            good_for: Default::default(),
            height: 6.0,
        };
        assert!(m.register(bad, &b).is_err());
        assert!(m.roll("nowhere", &b, 1).is_err());
        // No buildings and no props is nothing; no spacing is refused.
        let empty = MonumentTypeDef {
            name: "nothing".into(),
            slots: vec![],
            arrangement: Arrangement::Clustered,
            danger: 0,
            gated: false,
            suffix: "Nothing".into(),
            spacing: 50.0,
            props: vec![],
            good_for: Default::default(),
            height: 6.0,
        };
        assert!(m.register(empty, &b).is_err());
    }

    #[test]
    fn a_minor_site_rolls_props_and_a_loot_spot_with_no_buildings() {
        let (b, mut m) = registries();
        m.register(
            MonumentTypeDef {
                name: "wreck".into(),
                slots: vec![],
                arrangement: Arrangement::Clustered,
                danger: 0,
                gated: false,
                suffix: "Wreck".into(),
                spacing: 60.0,
                props: vec![PropSpec {
                    size: Vec3::new(4.0, 1.4, 1.8),
                    color: [0.4, 0.2, 0.15],
                    count: (1, 3),
                    radius: 5.0,
                }],
                good_for: Default::default(),
                height: 2.0,
            },
            &b,
        )
        .unwrap();
        for seed in 0..5u64 {
            let site = m.roll("wreck", &b, seed).unwrap();
            assert!(site.members.is_empty());
            assert_eq!(site.loot_spots.len(), 1, "one box at the origin");
            assert!(site.npc_spots.is_empty());
            assert!((1..=3).contains(&site.props.len()), "{} props", site.props.len());
            for prop in &site.props {
                let flat = prop.position.with_y(0.0).length();
                assert!(flat >= 1.0 && flat <= 5.0, "prop at {flat}");
                assert_eq!(prop.position.y, 0.7, "stands on the ground");
            }
            validate_monument(&site).unwrap();
        }
    }

    #[test]
    fn a_seed_rolls_the_same_building_twice_and_another_seed_differs() {
        let def = building("office", (2, 4), (1, 2), (2, 2), (0, 0));
        let a = generate_building(&def, &mut Roll::new(7));
        let b = generate_building(&def, &mut Roll::new(7));
        assert_eq!(a.rooms.len(), b.rooms.len());
        assert_eq!(a.rooms[0].interior, b.rooms[0].interior);
        let differs = (0..20u64).any(|seed| {
            let c = generate_building(&def, &mut Roll::new(seed));
            c.rooms.len() != a.rooms.len() || c.rooms[0].interior != a.rooms[0].interior
        });
        assert!(differs, "parameters must change between rolls");
    }

    #[test]
    fn generated_buildings_are_legal_and_within_their_ranges() {
        let (b, _) = registries();
        for name in ["shack", "office", "warehouse", "tower block", "bunker", "spire"] {
            let p = b.def(name).unwrap();
            for seed in 0..25u64 {
                let def = generate_building(p, &mut Roll::new(seed));
                validate(&def).unwrap();
                // Every room shares the floor height; the tower is
                // the one room taller than that.
                let height = def.rooms[0].interior.y;
                assert!(height >= p.height.0 && height < p.height.1 + 0.11, "{name} height");
                let levels: std::collections::BTreeSet<i32> = def
                    .rooms
                    .iter()
                    .filter(|r| r.interior.y <= height)
                    .map(|r| (r.origin.y / height).round() as i32)
                    .collect();
                let up = levels.iter().filter(|l| **l >= 0).count() as u32;
                let down = levels.iter().filter(|l| **l < 0).count() as u32;
                assert!(up >= p.floors.0 && up <= p.floors.1, "{name} floors {up}");
                assert!(down >= p.basements.0 && down <= p.basements.1, "{name} basements {down}");
                let total = up + down;
                let towers = def.rooms.iter().filter(|r| r.interior.y > height).count();
                assert_eq!(towers, usize::from(total > 1), "{name} stair tower");
                // Two flights per climb between neighbouring levels.
                assert_eq!(def.stairs.len(), 2 * (total as usize - 1), "{name} flights");
                if total > 1 {
                    let tower = def.rooms.iter().find(|r| r.interior.y > height).unwrap();
                    assert!((tower.interior.y - total as f32 * height).abs() < 1e-3);
                    assert!(tower.origin.y <= 0.0, "the tower starts at the deepest level");
                    // One doorway per level on its west wall.
                    assert_eq!(tower.openings.len(), total as usize, "{name} tower doorways");
                }
                let ground: Vec<_> = def
                    .rooms
                    .iter()
                    .filter(|r| r.origin.y == 0.0 && r.interior.y <= height)
                    .collect();
                let n = ground.len() as u32;
                let (lo, hi) = p.rooms_per_floor();
                assert!(n >= 1 && n <= hi, "{name} rooms {n}");
                let _ = lo; // carving may take rooms below the grid minimum
                assert!(
                    ground[0]
                        .openings
                        .iter()
                        .any(|o| o.side == Side::South && o.door),
                    "{name} has a front door"
                );
                // Every room reaches every neighbour: a doorway east
                // to the room beside it, north to the room behind it,
                // each authored on both sides.
                let touching = |a: &RoomSpec, b: &RoomSpec, side: Side| {
                    let d = b.origin - a.origin;
                    match side {
                        Side::East => d.z.abs() < 1e-3 && d.x > 0.0 && d.x < a.interior.x + b.interior.x,
                        Side::North => d.x.abs() < 1e-3 && d.z < 0.0 && -d.z < a.interior.z + b.interior.z,
                        _ => false,
                    }
                };
                for a in &ground {
                    for b in &ground {
                        if touching(a, b, Side::East) {
                            assert!(a.openings.iter().any(|o| o.side == Side::East && o.door));
                            assert!(b.openings.iter().any(|o| o.side == Side::West));
                        }
                        if touching(a, b, Side::North) {
                            assert!(a.openings.iter().any(|o| o.side == Side::North && o.door));
                            assert!(b.openings.iter().any(|o| o.side == Side::South));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_city_of_fifty_big_buildings_rolls_legal_and_large() {
        let (b, m) = registries();
        let city = m.roll("city", &b, 11).unwrap();
        validate_monument(&city).unwrap();
        assert_eq!(city.members.len(), 50);
        let rooms: usize = city.members.iter().map(|x| x.structure.rooms.len()).sum();
        assert!(rooms >= 50 * 16, "a city is thousands of rooms, got {rooms}");
        let blocks = city
            .members
            .iter()
            .filter(|x| x.structure.name == "tower block")
            .map(|x| x.structure.rooms.len())
            .min()
            .unwrap();
        assert!(blocks >= 12, "a tower block stays big after carving, got {blocks}");
    }

    #[test]
    fn rolled_monuments_are_legal_and_follow_their_type() {
        let (b, m) = registries();
        for name in ["roadside stop", "launch site", "city"] {
            let def = m.def(name).unwrap();
            for seed in 0..15u64 {
                let rolled = m.roll(name, &b, seed).unwrap();
                validate_monument(&rolled).unwrap();
                let min: u32 = def.slots.iter().map(|s| s.min).sum();
                let max: u32 = def.slots.iter().map(|s| s.max).sum();
                let n = rolled.members.len() as u32;
                assert!(n >= min && n <= max, "{name} rolled {n} buildings");
                assert_eq!(rolled.loot_spots.len(), rolled.members.len());
                assert_eq!(rolled.npc_spots.len(), rolled.members.len());
                assert!(rolled.name.ends_with(&def.suffix), "{}", rolled.name);
                if !def.gated {
                    assert!(rolled.gates.is_empty());
                }
                for member in &rolled.members {
                    assert!(
                        def.slots
                            .iter()
                            .any(|s| s.choices.contains(&member.structure.name)),
                        "{name} rolled a building outside its list"
                    );
                }
            }
        }
    }

    #[test]
    fn names_are_rolled_not_fixed() {
        let (b, m) = registries();
        let names: std::collections::HashSet<String> = (0..30u64)
            .map(|seed| m.roll("roadside stop", &b, seed).unwrap().name)
            .collect();
        assert!(names.len() > 3, "{names:?}");
        assert!(names.iter().all(|n| n.ends_with(" Stop")));
    }

    #[test]
    fn gated_monuments_gate_the_back_room_of_the_last_building() {
        let (b, m) = registries();
        let gated = (0..30u64)
            .map(|seed| m.roll("launch site", &b, seed).unwrap())
            .find(|r| !r.gates.is_empty())
            .expect("a launch site with a multi-room last building");
        let last = gated.members.last().unwrap();
        let back = back_room(&last.structure);
        assert!(back.origin.y == 0.0, "the back room is on the ground floor");
        assert!(
            back.interior.y <= last.structure.rooms[0].interior.y,
            "the back room is never the stairwell"
        );
        let gate = &gated.gates[0];
        let west = back
            .openings
            .iter()
            .find(|o| o.side == Side::West && o.sill == 0.0);
        let south = back
            .openings
            .iter()
            .find(|o| o.side == Side::South && o.sill == 0.0);
        let on_west = west.is_some_and(|o| {
            (gate.position.x
                - (last.offset.x + back.origin.x - back.interior.x / 2.0 - back.wall_thickness))
                .abs()
                < 1e-4
                && (gate.position.z - (last.offset.z + back.origin.z + o.offset)).abs() < 1e-4
        });
        let on_south = south.is_some_and(|o| {
            (gate.position.z
                - (last.offset.z + back.origin.z + back.interior.z / 2.0 + back.wall_thickness))
                .abs()
                < 1e-4
                && (gate.position.x - (last.offset.x + back.origin.x + o.offset)).abs() < 1e-4
        });
        assert!(on_west || on_south, "the gate sits on a back room doorway");
        assert_eq!(gate.level, 5);
        assert_eq!(gated.loot_spots.last().unwrap().danger, 6);
    }
}
