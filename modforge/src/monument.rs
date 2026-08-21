//! Monuments as topside design.md "Monuments" describes them: a
//! monument TYPE rolls which building types it includes from its
//! list, each building generates from parameters (room count,
//! floors, footprint, doors, windows, palette), the buildings sit
//! by an arrangement rule, loot spots, NPC spots, and gates come
//! with it, and even the name is random. Never the same twice.
//!
//! Engine-agnostic and deterministic: the same seed rolls the same
//! monument, so a world can be re-rolled and replayed. The output
//! is a [`MonumentDef`] of plain [`StructureDef`]s; the consumer's
//! spawner places them (topside: spawn_monument over
//! spawn_structure).
//!
//! Prior art: Rust's monuments (the types below are Rust's, adapted)
//! and its card rooms (the gates).

use glam::Vec3;

use crate::structure::{
    Aabb, Gate, LightSpec, LootSpot, MonumentDef, MonumentMember, NpcSpot, Opening, Rgb,
    RoomSpec, Side, SolidSpec, StairSpec, StructureDef, room_interior_aabb, validate,
};
use crate::unknown::rng;

const WALL: f32 = 0.2;
const DOOR_WIDTH: f32 = 1.2;
const WINDOW_WIDTH: f32 = 1.0;
const WINDOW_SILL: f32 = 1.0;
/// Stairwell interior width and the landing the flight ends on.
const STAIRWELL_WIDTH: f32 = 4.5;
const STAIR_WIDTH: f32 = 1.8;
const STAIR_LANDING: f32 = 0.5;
/// Step depth and max rise match the consumer's spawner (0.3 deep,
/// 0.31 max rise), so a flight's run is steps * 0.3.
const STEP_DEPTH: f32 = 0.3;
const STEP_RISE_MAX: f32 = 0.31;
const SLAB: f32 = 0.1;

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

/// The building types monuments are made of (design.md "Building
/// types"): small, medium, large.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuildingKind {
    // Small: one room, one floor.
    GasStation,
    ConvenienceStore,
    WaterWell,
    GuardTower,
    Shack,
    Lighthouse,
    // Medium: two to four rooms, one or two floors.
    Warehouse,
    OfficeBuilding,
    Workshop,
    Barracks,
    Lab,
    Supermarket,
    // Large: five or more rooms, two or more floors.
    Hangar,
    IndustrialPlant,
    Silo,
    DockBuilding,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuildingSize {
    Small,
    Medium,
    Large,
}

/// What varies on every roll of one building type (design.md,
/// operator-locked: "the buildings should have parameters that
/// change"). Rooms are per floor, laid in a row; floors stack.
#[derive(Clone, Copy, Debug)]
pub struct BuildingParams {
    pub size: BuildingSize,
    pub rooms: (u32, u32),
    pub floors: (u32, u32),
    pub width: (f32, f32),
    pub length: (f32, f32),
    pub height: f32,
    /// Chance per wall of a window, per mille.
    pub windows: u64,
    /// Chance per room of a piece of furniture, per mille.
    pub clutter: u64,
    pub palette: &'static [Rgb],
}

const CONCRETE: &[Rgb] = &[[0.45, 0.45, 0.47], [0.5, 0.48, 0.45], [0.38, 0.4, 0.42]];
const WOOD: &[Rgb] = &[[0.45, 0.32, 0.18], [0.55, 0.45, 0.2], [0.35, 0.25, 0.15]];
const STEEL: &[Rgb] = &[[0.3, 0.32, 0.35], [0.25, 0.3, 0.4], [0.5, 0.2, 0.15]];
const CLEAN: &[Rgb] = &[[0.8, 0.82, 0.85], [0.6, 0.7, 0.75], [0.3, 0.5, 0.6]];

impl BuildingKind {
    pub const ALL: [BuildingKind; 16] = [
        BuildingKind::GasStation,
        BuildingKind::ConvenienceStore,
        BuildingKind::WaterWell,
        BuildingKind::GuardTower,
        BuildingKind::Shack,
        BuildingKind::Lighthouse,
        BuildingKind::Warehouse,
        BuildingKind::OfficeBuilding,
        BuildingKind::Workshop,
        BuildingKind::Barracks,
        BuildingKind::Lab,
        BuildingKind::Supermarket,
        BuildingKind::Hangar,
        BuildingKind::IndustrialPlant,
        BuildingKind::Silo,
        BuildingKind::DockBuilding,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BuildingKind::GasStation => "gas station",
            BuildingKind::ConvenienceStore => "convenience store",
            BuildingKind::WaterWell => "water well",
            BuildingKind::GuardTower => "guard tower",
            BuildingKind::Shack => "shack",
            BuildingKind::Lighthouse => "lighthouse",
            BuildingKind::Warehouse => "warehouse",
            BuildingKind::OfficeBuilding => "office building",
            BuildingKind::Workshop => "workshop",
            BuildingKind::Barracks => "barracks",
            BuildingKind::Lab => "lab",
            BuildingKind::Supermarket => "supermarket",
            BuildingKind::Hangar => "hangar",
            BuildingKind::IndustrialPlant => "industrial plant",
            BuildingKind::Silo => "silo",
            BuildingKind::DockBuilding => "dock building",
        }
    }

    /// The parameter ranges for this type. Floors top out at two
    /// until multi-flight stairwells land.
    pub fn params(self) -> BuildingParams {
        use BuildingKind::*;
        use BuildingSize::*;
        let p = |size, rooms, floors, width, length, height, windows, clutter, palette| {
            BuildingParams {
                size,
                rooms,
                floors,
                width,
                length,
                height,
                windows,
                clutter,
                palette,
            }
        };
        match self {
            Shack => p(Small, (1, 1), (1, 1), (4.0, 6.0), (4.0, 6.0), 2.8, 400, 500, WOOD),
            GasStation => p(Small, (1, 2), (1, 1), (5.0, 8.0), (5.0, 7.0), 3.0, 600, 500, STEEL),
            ConvenienceStore => {
                p(Small, (2, 3), (1, 1), (5.0, 7.0), (6.0, 9.0), 3.0, 500, 700, CONCRETE)
            }
            WaterWell => p(Small, (1, 1), (1, 1), (3.5, 4.5), (3.5, 4.5), 3.0, 0, 300, CONCRETE),
            GuardTower => p(Small, (1, 1), (2, 2), (3.5, 4.0), (5.0, 6.0), 3.0, 800, 200, WOOD),
            Lighthouse => p(Small, (1, 1), (2, 2), (4.0, 5.0), (5.0, 6.0), 3.0, 500, 200, CLEAN),
            Warehouse => {
                p(Medium, (2, 3), (1, 1), (8.0, 12.0), (10.0, 16.0), 5.0, 300, 800, STEEL)
            }
            OfficeBuilding => {
                p(Medium, (3, 4), (2, 2), (5.0, 6.0), (6.0, 8.0), 3.0, 800, 600, CONCRETE)
            }
            Workshop => p(Medium, (2, 3), (1, 1), (6.0, 8.0), (7.0, 10.0), 4.0, 400, 900, STEEL),
            Barracks => p(Medium, (3, 4), (1, 1), (5.0, 6.0), (7.0, 9.0), 3.0, 500, 700, WOOD),
            Lab => p(Medium, (2, 4), (1, 2), (5.0, 7.0), (6.0, 8.0), 3.0, 300, 800, CLEAN),
            Supermarket => {
                p(Medium, (2, 3), (1, 1), (10.0, 14.0), (10.0, 14.0), 4.0, 200, 900, CONCRETE)
            }
            Hangar => p(Large, (2, 3), (1, 1), (14.0, 20.0), (20.0, 30.0), 8.0, 100, 600, STEEL),
            IndustrialPlant => {
                p(Large, (4, 6), (2, 2), (8.0, 10.0), (10.0, 12.0), 5.0, 300, 900, STEEL)
            }
            Silo => p(Large, (2, 3), (2, 2), (5.0, 6.0), (5.0, 6.0), 3.0, 100, 500, CONCRETE),
            DockBuilding => {
                p(Large, (3, 5), (1, 1), (6.0, 8.0), (14.0, 20.0), 4.0, 400, 700, WOOD)
            }
        }
    }

    pub fn size(self) -> BuildingSize {
        self.params().size
    }
}

/// Roll one building of `kind`: a row of rooms per floor, doors
/// between neighbours and one out the south of the first room,
/// windows by chance, a full-height stairwell at the east end when
/// there is a second floor, furniture and a light per room. Every
/// def passes [`validate`].
pub fn generate_building(kind: BuildingKind, roll: &mut Roll) -> StructureDef {
    let p = kind.params();
    let per_floor = roll.between(p.rooms.0, p.rooms.1);
    let floors = roll.between(p.floors.0, p.floors.1);
    let length = roll.measure(p.length.0, p.length.1);
    let widths: Vec<f32> = (0..per_floor)
        .map(|_| roll.measure(p.width.0, p.width.1))
        .collect();

    // Room origins along x: neighbours are 2 wall thicknesses apart
    // (each room builds its own wall on the shared plane).
    let mut xs = Vec::with_capacity(widths.len());
    let mut cursor = 0.0;
    for (i, w) in widths.iter().enumerate() {
        if i > 0 {
            cursor += widths[i - 1] / 2.0 + 2.0 * WALL + w / 2.0;
        }
        xs.push(cursor);
    }
    let last = widths.len() - 1;
    let stairwell_x = xs[last] + widths[last] / 2.0 + 2.0 * WALL + STAIRWELL_WIDTH / 2.0;

    let mut rooms = Vec::new();
    let mut furniture = Vec::new();
    let mut lights = Vec::new();
    for floor in 0..floors {
        let y = floor as f32 * p.height;
        for (i, (&x, &w)) in xs.iter().zip(&widths).enumerate() {
            let mut openings = Vec::new();
            // Exterior door out the south of the first ground room.
            if i == 0 && floor == 0 {
                openings.push(door(Side::South, 0.0));
            }
            // Doors between neighbours: the west room owns the hinge.
            if i > 0 {
                openings.push(Opening {
                    door: false,
                    ..door(Side::West, 0.0)
                });
            }
            if i < last {
                openings.push(door(Side::East, 0.0));
            } else if floors > 1 {
                // Into the stairwell, at this floor's level.
                openings.push(Opening {
                    door: false,
                    ..door(Side::East, 0.0)
                });
            }
            // Windows where a door does not already sit.
            for side in [Side::North, Side::South] {
                let taken = side == Side::South && i == 0 && floor == 0;
                if !taken && w >= 2.0 * WINDOW_WIDTH + 1.0 && roll.chance(p.windows) {
                    openings.push(window(side, 0.0));
                }
            }
            if i == 0 && length >= 2.0 * WINDOW_WIDTH + 1.0 && roll.chance(p.windows) {
                openings.push(window(Side::West, 0.0));
            }
            let origin = Vec3::new(x, y, 0.0);
            rooms.push(RoomSpec {
                origin,
                interior: Vec3::new(w, p.height, length),
                wall_thickness: WALL,
                openings,
                floor: true,
                ceiling: floor + 1 == floors,
            });
            if roll.chance(p.clutter) {
                let size = Vec3::new(
                    roll.measure(0.6, 1.4),
                    roll.measure(0.4, 1.0),
                    roll.measure(0.6, 1.2),
                );
                // Against the north wall, clear of every doorway.
                let dx = roll.measure(-(w / 2.0 - 1.0).max(0.0), (w / 2.0 - 1.0).max(0.1));
                furniture.push(SolidSpec {
                    center: origin + Vec3::new(dx, size.y / 2.0, -length / 2.0 + size.z / 2.0 + 0.3),
                    size,
                    color: *roll.pick(p.palette),
                });
            }
            lights.push(LightSpec {
                position: origin + Vec3::new(0.0, p.height - 0.4, 0.0),
                color: [1.0, 0.9, 0.7],
                intensity: 400_000.0,
            });
        }
    }

    // The stairwell: one full-height room east of the row with a
    // flight from the ground floor to the first, its landing at
    // the first floor's doorway.
    let mut stairs = Vec::new();
    if floors > 1 {
        let total = floors as f32 * p.height;
        let mut openings = vec![Opening {
            door: false,
            ..door(Side::West, 0.0)
        }];
        for floor in 1..floors {
            openings.push(Opening {
                side: Side::West,
                offset: 0.0,
                width: STAIR_WIDTH,
                sill: floor as f32 * p.height + SLAB,
                door: false,
            });
        }
        rooms.push(RoomSpec {
            origin: Vec3::new(stairwell_x, 0.0, 0.0),
            interior: Vec3::new(STAIRWELL_WIDTH, total, length),
            wall_thickness: WALL,
            openings,
            floor: true,
            ceiling: true,
        });
        // The flight runs north along the stairwell's east half and
        // tops out with its landing centred on the doorway (z 0).
        let steps = (p.height / STEP_RISE_MAX).ceil().max(1.0);
        let run = steps * STEP_DEPTH;
        let base_z = run + STAIR_LANDING / 2.0;
        stairs.push(StairSpec {
            base: Vec3::new(stairwell_x, 0.0, base_z),
            side: Side::North,
            width: STAIR_WIDTH,
            rise: p.height + SLAB,
            landing: STAIR_LANDING,
        });
        lights.push(LightSpec {
            position: Vec3::new(stairwell_x, total - 0.6, 0.0),
            color: [1.0, 0.9, 0.7],
            intensity: 400_000.0,
        });
    }

    let def = StructureDef {
        name: kind.label().to_string(),
        rooms,
        stairs,
        furniture,
        lights,
    };
    debug_assert!(validate(&def).is_ok(), "generated buildings are legal");
    def
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
/// its walls, flattened to the ground.
pub fn footprint(def: &StructureDef) -> Aabb {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for room in &def.rooms {
        let a = room_interior_aabb(room);
        let t = Vec3::new(room.wall_thickness, 0.0, room.wall_thickness);
        min = min.min(a.min - t);
        max = max.max(a.max + t);
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
/// buildings drawn from `choices`.
#[derive(Clone, Copy, Debug)]
pub struct BuildingSlot {
    pub choices: &'static [BuildingKind],
    pub min: u32,
    pub max: u32,
}

/// What one monument type rolls from.
#[derive(Clone, Copy, Debug)]
pub struct MonumentRules {
    pub slots: &'static [BuildingSlot],
    pub arrangement: Arrangement,
    /// Base danger of its loot and NPC spots (design.md "Danger").
    pub danger: u32,
    /// Whether its last building hides a gated back room.
    pub gated: bool,
    /// The last word of its rolled name.
    pub suffix: &'static str,
}

/// The monument types (design.md "Monument types", Rust's adapted).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MonumentKind {
    RoadsideStop,
    Airfield,
    PowerPlant,
    TrainYard,
    MilitaryOutpost,
    ResearchStation,
    Harbor,
    Junkyard,
    Sewer,
    LaunchSite,
}

const SMALL: &[BuildingKind] = &[
    BuildingKind::GasStation,
    BuildingKind::ConvenienceStore,
    BuildingKind::WaterWell,
    BuildingKind::GuardTower,
    BuildingKind::Shack,
    BuildingKind::Lighthouse,
];
const MEDIUM: &[BuildingKind] = &[
    BuildingKind::Warehouse,
    BuildingKind::OfficeBuilding,
    BuildingKind::Workshop,
    BuildingKind::Barracks,
    BuildingKind::Lab,
    BuildingKind::Supermarket,
];
const SMALL_OR_MEDIUM: &[BuildingKind] = &[
    BuildingKind::GasStation,
    BuildingKind::Shack,
    BuildingKind::GuardTower,
    BuildingKind::Warehouse,
    BuildingKind::Workshop,
    BuildingKind::OfficeBuilding,
];

const fn slot(choices: &'static [BuildingKind], min: u32, max: u32) -> BuildingSlot {
    BuildingSlot { choices, min, max }
}

const ROADSIDE_STOP: &[BuildingSlot] = &[slot(SMALL, 1, 2)];
const AIRFIELD: &[BuildingSlot] = &[
    slot(&[BuildingKind::Hangar], 1, 1),
    slot(SMALL_OR_MEDIUM, 1, 2),
];
const POWER_PLANT: &[BuildingSlot] = &[
    slot(&[BuildingKind::IndustrialPlant], 1, 1),
    slot(MEDIUM, 1, 3),
];
const TRAIN_YARD: &[BuildingSlot] = &[
    slot(&[BuildingKind::Warehouse], 1, 2),
    slot(SMALL, 1, 2),
];
const MILITARY_OUTPOST: &[BuildingSlot] = &[
    slot(&[BuildingKind::Barracks], 1, 1),
    slot(&[BuildingKind::GuardTower], 1, 2),
];
const RESEARCH_STATION: &[BuildingSlot] = &[
    slot(&[BuildingKind::Lab], 1, 1),
    slot(MEDIUM, 1, 2),
];
const HARBOR: &[BuildingSlot] = &[
    slot(&[BuildingKind::DockBuilding], 1, 1),
    slot(&[BuildingKind::Warehouse], 1, 2),
];
const JUNKYARD: &[BuildingSlot] = &[slot(&[BuildingKind::Shack, BuildingKind::Workshop], 2, 3)];
const SEWER: &[BuildingSlot] = &[
    slot(&[BuildingKind::Silo], 1, 1),
    slot(&[BuildingKind::Shack], 1, 1),
];
const LAUNCH_SITE: &[BuildingSlot] = &[
    slot(&[BuildingKind::Hangar], 1, 1),
    slot(&[BuildingKind::IndustrialPlant], 1, 1),
    slot(MEDIUM, 1, 2),
];

impl MonumentKind {
    pub const ALL: [MonumentKind; 10] = [
        MonumentKind::RoadsideStop,
        MonumentKind::Airfield,
        MonumentKind::PowerPlant,
        MonumentKind::TrainYard,
        MonumentKind::MilitaryOutpost,
        MonumentKind::ResearchStation,
        MonumentKind::Harbor,
        MonumentKind::Junkyard,
        MonumentKind::Sewer,
        MonumentKind::LaunchSite,
    ];

    pub fn rules(self) -> MonumentRules {
        use Arrangement::*;
        use MonumentKind::*;
        let r = |slots, arrangement, danger, gated, suffix| MonumentRules {
            slots,
            arrangement,
            danger,
            gated,
            suffix,
        };
        match self {
            RoadsideStop => r(ROADSIDE_STOP, AlongRoad, 1, false, "Stop"),
            Airfield => r(AIRFIELD, AlongRoad, 3, true, "Airfield"),
            PowerPlant => r(POWER_PLANT, Clustered, 4, true, "Plant"),
            TrainYard => r(TRAIN_YARD, AlongRoad, 2, false, "Yard"),
            MilitaryOutpost => r(MILITARY_OUTPOST, AroundYard, 3, true, "Outpost"),
            ResearchStation => r(RESEARCH_STATION, AroundYard, 3, true, "Station"),
            Harbor => r(HARBOR, AlongRoad, 2, false, "Harbor"),
            Junkyard => r(JUNKYARD, Clustered, 1, false, "Junkyard"),
            Sewer => r(SEWER, Clustered, 3, true, "Sewer"),
            LaunchSite => r(LAUNCH_SITE, AroundYard, 5, true, "Site"),
        }
    }
}

const NAME_FIRST: &[&str] = &[
    "Ash", "Rust", "Hollow", "Grey", "Salt", "Cinder", "Dust", "Black", "Cold", "Broken", "Iron",
    "Sour", "Dead", "Low", "Far",
];
const NAME_SECOND: &[&str] = &[
    "fall", "ridge", "water", "field", "mark", "reach", "pass", "well", "brook", "gate", "fork",
    "hill", "marsh", "bend", "row",
];

/// A rolled monument name: two joined words and the type's suffix
/// ("Ashfall Stop"). Random every time (design.md: "even the
/// monument NAME is random").
pub fn roll_name(kind: MonumentKind, roll: &mut Roll) -> String {
    format!(
        "{}{} {}",
        roll.pick(NAME_FIRST),
        roll.pick(NAME_SECOND),
        kind.rules().suffix
    )
}

/// Roll one monument of `kind` from `seed`: pick building types per
/// the rules, generate each building, arrange them, then place loot
/// spots (one per building), NPC spots (one outside each door), and
/// the gate (the last building's back room, when gated).
pub fn roll_monument(kind: MonumentKind, seed: u64) -> MonumentDef {
    let rules = kind.rules();
    let mut roll = Roll::new(seed);
    let name = roll_name(kind, &mut roll);

    let mut buildings = Vec::new();
    for slot in rules.slots {
        let count = roll.between(slot.min, slot.max);
        for _ in 0..count {
            let kind = *roll.pick(slot.choices);
            buildings.push(generate_building(kind, &mut roll));
        }
    }

    let members = arrange(buildings, rules.arrangement, &mut roll);

    let mut loot_spots = Vec::new();
    let mut npc_spots = Vec::new();
    let mut gates = Vec::new();
    let count = members.len();
    for (i, member) in members.iter().enumerate() {
        let first = &member.structure.rooms[0];
        let back = member.structure.rooms[member.structure.rooms.len() - 1].clone();
        let gated_here = rules.gated && i + 1 == count && member.structure.rooms.len() > 1;
        let danger = rules.danger + u32::from(gated_here);
        // Loot inside the back room; NPC outside the front door.
        loot_spots.push(LootSpot {
            position: member.offset + back.origin + Vec3::new(0.0, 0.35, 0.0),
            danger,
        });
        npc_spots.push(NpcSpot {
            position: member.offset + first.origin + Vec3::new(0.0, 0.0, first.interior.z / 2.0 + 2.0),
            danger: rules.danger,
        });
        if gated_here {
            // The door between the back room and its west neighbour.
            gates.push(Gate {
                position: member.offset
                    + back.origin
                    + Vec3::new(-(back.interior.x / 2.0 + back.wall_thickness), 0.0, 0.0),
                level: rules.danger,
            });
        }
    }

    MonumentDef {
        name,
        members,
        loot_spots,
        npc_spots,
        gates,
    }
}

/// Place buildings by the rule without footprints overlapping.
/// Along a road: alternating sides of a road along x. Clustered:
/// packed around the origin. Around a yard: on a ring leaving the
/// middle open. Each placement pushes outward until it is clear of
/// the ones before it.
fn arrange(buildings: Vec<StructureDef>, arrangement: Arrangement, roll: &mut Roll) -> Vec<MonumentMember> {
    const GAP: f32 = 4.0;
    let mut placed: Vec<Aabb> = Vec::new();
    let mut members = Vec::new();
    for (i, structure) in buildings.into_iter().enumerate() {
        let foot = footprint(&structure);
        let size = foot.max - foot.min;
        let centre = (foot.min + foot.max) / 2.0;
        let mut offset = Vec3::ZERO;
        for attempt in 0..16u32 {
            let reach = attempt as f32 * GAP;
            let candidate = match arrangement {
                Arrangement::AlongRoad => {
                    let side = if i % 2 == 0 { 1.0 } else { -1.0 };
                    let along = (i / 2) as f32 * (size.x + GAP * 2.0) + reach;
                    Vec3::new(along, 0.0, side * (6.0 + size.z / 2.0))
                }
                Arrangement::Clustered => {
                    let angle = (i as f32 * 2.4 + roll.measure(0.0, 0.6)) as f32;
                    let radius = if i == 0 { 0.0 } else { size.length() / 2.0 + GAP + reach };
                    Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius)
                }
                Arrangement::AroundYard => {
                    let angle = i as f32 * 2.4 + roll.measure(0.0, 0.6);
                    let radius = 12.0 + size.length() / 2.0 + reach;
                    Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius)
                }
            } - centre.with_y(0.0);
            let here = shifted(&foot, candidate);
            let clear = placed.iter().all(|p| !grown(p, GAP / 2.0).overlaps(&here));
            if clear {
                offset = candidate;
                break;
            }
            offset = candidate;
        }
        placed.push(shifted(&foot, offset));
        members.push(MonumentMember { structure, offset });
    }
    members
}

fn grown(a: &Aabb, by: f32) -> Aabb {
    Aabb {
        min: a.min - Vec3::new(by, 0.0, by),
        max: a.max + Vec3::new(by, 0.0, by),
    }
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

    #[test]
    fn a_seed_rolls_the_same_building_twice_and_another_seed_differs() {
        let a = generate_building(BuildingKind::Warehouse, &mut Roll::new(7));
        let b = generate_building(BuildingKind::Warehouse, &mut Roll::new(7));
        assert_eq!(a.rooms.len(), b.rooms.len());
        assert_eq!(a.rooms[0].interior, b.rooms[0].interior);
        let differs = (0..20u64).any(|seed| {
            let c = generate_building(BuildingKind::Warehouse, &mut Roll::new(seed));
            c.rooms.len() != a.rooms.len() || c.rooms[0].interior != a.rooms[0].interior
        });
        assert!(differs, "parameters must change between rolls");
    }

    #[test]
    fn every_building_kind_generates_legal_defs_within_its_ranges() {
        for kind in BuildingKind::ALL {
            let p = kind.params();
            for seed in 0..25u64 {
                let def = generate_building(kind, &mut Roll::new(seed));
                validate(&def).unwrap();
                let floors = def
                    .rooms
                    .iter()
                    .map(|r| (r.origin.y / p.height).round() as u32 + 1)
                    .max()
                    .unwrap();
                assert!(floors >= p.floors.0 && floors <= p.floors.1, "{kind:?} floors {floors}");
                let stairwells = def.rooms.iter().filter(|r| r.interior.y > p.height).count();
                assert_eq!(stairwells, usize::from(floors > 1), "{kind:?} stairwell");
                assert_eq!(def.stairs.len(), usize::from(floors > 1), "{kind:?} stairs");
                let ground: Vec<_> = def
                    .rooms
                    .iter()
                    .filter(|r| r.origin.y == 0.0 && r.interior.y <= p.height)
                    .collect();
                let n = ground.len() as u32;
                assert!(n >= p.rooms.0 && n <= p.rooms.1, "{kind:?} rooms {n}");
                assert!(
                    ground[0]
                        .openings
                        .iter()
                        .any(|o| o.side == Side::South && o.door),
                    "{kind:?} has a front door"
                );
                // Every neighbouring pair authors both sides of its doorway.
                for pair in ground.windows(2) {
                    assert!(pair[0].openings.iter().any(|o| o.side == Side::East));
                    assert!(pair[1].openings.iter().any(|o| o.side == Side::West));
                }
            }
        }
    }

    #[test]
    fn every_monument_kind_rolls_legal_monuments_per_its_rules() {
        for kind in MonumentKind::ALL {
            let rules = kind.rules();
            for seed in 0..15u64 {
                let m = roll_monument(kind, seed);
                validate_monument(&m).unwrap();
                let min: u32 = rules.slots.iter().map(|s| s.min).sum();
                let max: u32 = rules.slots.iter().map(|s| s.max).sum();
                let n = m.members.len() as u32;
                assert!(n >= min && n <= max, "{kind:?} rolled {n} buildings");
                assert_eq!(m.loot_spots.len(), m.members.len(), "one loot spot per building");
                assert_eq!(m.npc_spots.len(), m.members.len(), "one npc spot per building");
                assert!(m.name.ends_with(rules.suffix), "{}", m.name);
                if !rules.gated {
                    assert!(m.gates.is_empty());
                }
            }
        }
    }

    #[test]
    fn roadside_stop_is_one_or_two_small_buildings_along_a_road() {
        let m = roll_monument(MonumentKind::RoadsideStop, 3);
        assert!((1..=2).contains(&m.members.len()));
        for member in &m.members {
            let kind = BuildingKind::ALL
                .iter()
                .find(|k| k.label() == member.structure.name)
                .unwrap();
            assert_eq!(kind.size(), BuildingSize::Small);
        }
    }

    #[test]
    fn names_are_rolled_not_fixed() {
        let names: std::collections::HashSet<String> = (0..30u64)
            .map(|seed| roll_monument(MonumentKind::Junkyard, seed).name)
            .collect();
        assert!(names.len() > 5, "{names:?}");
        assert!(names.iter().all(|n| n.ends_with(" Junkyard")));
    }

    #[test]
    fn gated_monuments_gate_the_back_room_of_the_last_building() {
        let gated = (0..30u64)
            .map(|seed| roll_monument(MonumentKind::LaunchSite, seed))
            .find(|m| !m.gates.is_empty())
            .expect("a launch site with a multi-room last building");
        let last = gated.members.last().unwrap();
        let back = last.structure.rooms[last.structure.rooms.len() - 1].clone();
        let gate = &gated.gates[0];
        let expected_x = last.offset.x + back.origin.x - back.interior.x / 2.0 - back.wall_thickness;
        assert!((gate.position.x - expected_x).abs() < 1e-4);
        assert_eq!(gate.level, MonumentKind::LaunchSite.rules().danger);
        assert_eq!(gated.loot_spots.last().unwrap().danger, gate.level + 1);
    }
}
