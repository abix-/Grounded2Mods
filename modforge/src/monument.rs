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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuildingSize {
    Small,
    Medium,
    Large,
}

/// One building type as data (design.md "Building types"): what
/// varies on every roll (operator-locked: "the buildings should
/// have parameters that change"). Rooms are per floor, laid in a
/// row; floors stack. `name` is the id.
#[derive(Clone, Debug)]
pub struct BuildingTypeDef {
    pub name: String,
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
    pub palette: Vec<Rgb>,
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
        if def.floors.1 > 2 {
            return Err(format!(
                "building type '{}': floors top out at two until multi-flight stairwells land",
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

/// Roll one building of type `def`: a row of rooms per floor, doors
/// between neighbours and one out the south of the first room,
/// windows by chance, a full-height stairwell at the east end when
/// there is a second floor, furniture and a light per room. Every
/// result passes [`validate`].
pub fn generate_building(def: &BuildingTypeDef, roll: &mut Roll) -> StructureDef {
    let p = def;
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
                    center: origin
                        + Vec3::new(dx, size.y / 2.0, -length / 2.0 + size.z / 2.0 + 0.3),
                    size,
                    color: *roll.pick(&p.palette),
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
        // The flight runs north and tops out with its landing
        // centred on the doorway (z 0).
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

    let out = StructureDef {
        name: def.name.clone(),
        rooms,
        stairs,
        furniture,
        lights,
    };
    debug_assert!(validate(&out).is_ok(), "generated buildings are legal");
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
/// buildings drawn from `choices` (building type names).
#[derive(Clone, Debug)]
pub struct BuildingSlot {
    pub choices: Vec<String>,
    pub min: u32,
    pub max: u32,
}

/// One monument type as data (design.md "Monument types"). `name`
/// is the id; `suffix` ends every rolled monument's name.
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
        if def.slots.is_empty() {
            return Err(format!("monument type '{}' has no building slots", def.name));
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
            let back = member.structure.rooms[member.structure.rooms.len() - 1].clone();
            let gated_here = def.gated && i + 1 == count && member.structure.rooms.len() > 1;
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
            if gated_here {
                // The door between the back room and its west neighbour.
                gates.push(Gate {
                    position: member.offset
                        + back.origin
                        + Vec3::new(-(back.interior.x / 2.0 + back.wall_thickness), 0.0, 0.0),
                    level: def.danger,
                });
            }
        }

        Ok(MonumentDef {
            name: rolled_name,
            members,
            loot_spots,
            npc_spots,
            gates,
        })
    }
}

/// Place buildings by the rule without footprints overlapping.
/// Along a road: alternating sides of a road along x. Clustered:
/// packed around the origin. Around a yard: on a ring leaving the
/// middle open. Each placement pushes outward until it is clear of
/// the ones before it.
fn arrange(
    buildings: Vec<StructureDef>,
    arrangement: Arrangement,
    roll: &mut Roll,
) -> Vec<MonumentMember> {
    const GAP: f32 = 4.0;
    let mut placed: Vec<Aabb> = Vec::new();
    let mut members = Vec::new();
    for (i, structure) in buildings.into_iter().enumerate() {
        let foot = footprint(&structure);
        let size = foot.max - foot.min;
        let centre = (foot.min + foot.max) / 2.0;
        let jitter = roll.measure(0.0, 0.6);
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
                    let angle = i as f32 * 2.4 + jitter;
                    let radius = if i == 0 {
                        0.0
                    } else {
                        size.length() / 2.0 + GAP + reach
                    };
                    Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius)
                }
                Arrangement::AroundYard => {
                    let angle = i as f32 * 2.4 + jitter;
                    let radius = 12.0 + size.length() / 2.0 + reach;
                    Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius)
                }
            } - centre.with_y(0.0);
            let here = shifted(&foot, candidate);
            offset = candidate;
            if placed.iter().all(|p| !grown(p, GAP / 2.0).overlaps(&here)) {
                break;
            }
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

    fn building(name: &str, rooms: (u32, u32), floors: (u32, u32)) -> BuildingTypeDef {
        BuildingTypeDef {
            name: name.to_string(),
            size: BuildingSize::Small,
            rooms,
            floors,
            width: (4.0, 8.0),
            length: (5.0, 9.0),
            height: 3.0,
            windows: 500,
            clutter: 600,
            palette: vec![[0.5, 0.4, 0.3]],
        }
    }

    fn registries() -> (BuildingRegistry, MonumentRegistry) {
        let mut b = BuildingRegistry::default();
        b.register(building("shack", (1, 1), (1, 1))).unwrap();
        b.register(building("office", (2, 4), (2, 2))).unwrap();
        b.register(building("warehouse", (2, 3), (1, 1))).unwrap();
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
            },
            &b,
        )
        .unwrap();
        (b, m)
    }

    #[test]
    fn registries_reject_unknown_buildings_and_duplicates() {
        let (mut b, mut m) = registries();
        assert!(b.register(building("shack", (1, 1), (1, 1))).is_err());
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
        };
        assert!(m.register(bad, &b).is_err());
        assert!(m.roll("nowhere", &b, 1).is_err());
    }

    #[test]
    fn a_seed_rolls_the_same_building_twice_and_another_seed_differs() {
        let def = building("office", (2, 4), (2, 2));
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
        for name in ["shack", "office", "warehouse"] {
            let p = b.def(name).unwrap();
            for seed in 0..25u64 {
                let def = generate_building(p, &mut Roll::new(seed));
                validate(&def).unwrap();
                let floors = def
                    .rooms
                    .iter()
                    .map(|r| (r.origin.y / p.height).round() as u32 + 1)
                    .max()
                    .unwrap();
                assert!(floors >= p.floors.0 && floors <= p.floors.1, "{name} floors {floors}");
                let stairwells = def.rooms.iter().filter(|r| r.interior.y > p.height).count();
                assert_eq!(stairwells, usize::from(floors > 1), "{name} stairwell");
                assert_eq!(def.stairs.len(), usize::from(floors > 1), "{name} stairs");
                let ground: Vec<_> = def
                    .rooms
                    .iter()
                    .filter(|r| r.origin.y == 0.0 && r.interior.y <= p.height)
                    .collect();
                let n = ground.len() as u32;
                assert!(n >= p.rooms.0 && n <= p.rooms.1, "{name} rooms {n}");
                assert!(
                    ground[0]
                        .openings
                        .iter()
                        .any(|o| o.side == Side::South && o.door),
                    "{name} has a front door"
                );
                for pair in ground.windows(2) {
                    assert!(pair[0].openings.iter().any(|o| o.side == Side::East));
                    assert!(pair[1].openings.iter().any(|o| o.side == Side::West));
                }
            }
        }
    }

    #[test]
    fn rolled_monuments_are_legal_and_follow_their_type() {
        let (b, m) = registries();
        for name in ["roadside stop", "launch site"] {
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
        let back = last.structure.rooms[last.structure.rooms.len() - 1].clone();
        let gate = &gated.gates[0];
        let expected_x =
            last.offset.x + back.origin.x - back.interior.x / 2.0 - back.wall_thickness;
        assert!((gate.position.x - expected_x).abs() < 1e-4);
        assert_eq!(gate.level, 5);
        assert_eq!(gated.loot_spots.last().unwrap().danger, 6);
    }
}
