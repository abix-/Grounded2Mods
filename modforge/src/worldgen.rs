//! The world as data (topside design.md "The world"): a `WorldDef`
//! rolls a `World` from a seed, the way a MonumentTypeDef rolls a
//! monument. The roll decides the heightmap, a biome per cell,
//! monument sites, and roads between them; the consumer builds the
//! scene from it and the storm rolls a new one through the same
//! path. Engine-free: glam math, the `noise` crate, plain data.
//!
//! Prior art, read from source: Endless's worldgen.rs (continents
//! and world map styles: layered simplex elevation, edge falloff,
//! moisture, rejection-sampled sites with spacing) on Red Blob
//! Games' "Making maps with noise"; Rust's roads laid between
//! monuments over the heightmap by easy slope.

use std::collections::{BinaryHeap, HashMap};

use glam::Vec2;
use noise::{NoiseFn, Simplex};

use crate::biome::BiomeRegistry;
use crate::monument::Roll;

/// One rule mapping a band of height and moisture to a biome name.
/// The first rule that matches a cell wins, so order them from the
/// most specific to the catch-all.
#[derive(Clone, Debug, PartialEq)]
pub struct BiomeRule {
    pub biome: String,
    /// Height as a fraction of the world's height scale, 0 to 1.
    pub height: (f32, f32),
    /// Moisture 0 to 1.
    pub moisture: (f32, f32),
}

/// What to roll: the world's shape and its rules, never a result.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldDef {
    /// Side of the square world in metres.
    pub size: f32,
    /// Metres per heightmap cell.
    pub cell: f32,
    /// Metres from the lowest to the highest ground of the base
    /// terrain, before mountains.
    pub height_scale: f32,
    /// Metres per noise period for the largest feature.
    pub feature_size: f32,
    /// Shaping (Rust's map: flat lowlands, ridged mountain ranges in
    /// some regions, stepped plateaus in others). `flatness` is the
    /// power the base noise is raised to (1 is plain fBm; 2 flattens
    /// the low ground into plains and valleys). `mountain_height` is
    /// the extra metres ridged mountains add where the mountain mask
    /// is high; `plateau_step` is the terrace height where the
    /// plateau mask is high (0 for none).
    pub flatness: f32,
    pub mountain_height: f32,
    pub plateau_step: f32,
    /// Below this fraction of height_scale the cell is water.
    pub water_level: f32,
    pub biome_rules: Vec<BiomeRule>,
    /// Monument types to place, as (type name, how many). Each type's
    /// own `spacing` keeps it off other sites and the bunker: two
    /// sites may stand as close as the smaller of their spacings, so
    /// a wreck can sit near a city but two cities never touch.
    pub monuments: Vec<(String, u32)>,
    /// Where the bunker is; the world rolls around it and roads lead
    /// there. Flat ground is kept in `bunker_clear` around it.
    pub bunker: Vec2,
    pub bunker_clear: f32,
}

/// One placed monument, type and centre, in world metres.
#[derive(Clone, Debug, PartialEq)]
pub struct Site {
    pub monument: String,
    pub position: Vec2,
    pub biome: String,
    /// The type's spacing, kept so the consumer knows how much ground
    /// around the site is the site's.
    pub spacing: f32,
}

/// A road: a polyline of cell centres from a site (`site` indexes
/// `World::sites`) to the bunker or an earlier-connected site.
#[derive(Clone, Debug, PartialEq)]
pub struct Road {
    pub site: usize,
    pub points: Vec<Vec2>,
}

/// The rolled world: all data, no engine.
#[derive(Clone, Debug, PartialEq)]
pub struct World {
    pub seed: u64,
    pub def: WorldDef,
    /// Cells per side.
    pub cells: usize,
    /// Height in metres, row-major, `cells * cells`. Zero is the
    /// ground at the bunker: the whole map is shifted so the bunker
    /// built at height zero stands on its floor.
    pub heights: Vec<f32>,
    /// Below this height a cell is water (the def's water level,
    /// shifted with the map).
    pub sea_level: f32,
    /// Biome name per cell, as an index into `biomes`.
    pub biome_of_cell: Vec<u16>,
    pub biomes: Vec<String>,
    pub sites: Vec<Site>,
    pub roads: Vec<Road>,
}

impl World {
    pub fn index(&self, col: usize, row: usize) -> usize {
        row * self.cells + col
    }

    pub fn height_at_cell(&self, col: usize, row: usize) -> f32 {
        self.heights[self.index(col, row)]
    }

    pub fn biome_at_cell(&self, col: usize, row: usize) -> &str {
        &self.biomes[self.biome_of_cell[self.index(col, row)] as usize]
    }

    /// Cell holding a world position (clamped to the edge).
    pub fn cell_of(&self, p: Vec2) -> (usize, usize) {
        let half = self.def.size / 2.0;
        let max = self.cells - 1;
        let col = (((p.x + half) / self.def.cell) as isize).clamp(0, max as isize) as usize;
        let row = (((p.y + half) / self.def.cell) as isize).clamp(0, max as isize) as usize;
        (col, row)
    }

    /// Centre of a cell in world metres (x east, y is the world's z).
    pub fn cell_center(&self, col: usize, row: usize) -> Vec2 {
        let half = self.def.size / 2.0;
        Vec2::new(
            col as f32 * self.def.cell - half + self.def.cell / 2.0,
            row as f32 * self.def.cell - half + self.def.cell / 2.0,
        )
    }

    /// Ground height at a world position, bilinear between cells.
    pub fn height_at(&self, p: Vec2) -> f32 {
        let half = self.def.size / 2.0;
        let fx = ((p.x + half) / self.def.cell - 0.5).clamp(0.0, (self.cells - 1) as f32);
        let fy = ((p.y + half) / self.def.cell - 0.5).clamp(0.0, (self.cells - 1) as f32);
        let (c0, r0) = (fx.floor() as usize, fy.floor() as usize);
        let (c1, r1) = ((c0 + 1).min(self.cells - 1), (r0 + 1).min(self.cells - 1));
        let (tx, ty) = (fx - c0 as f32, fy - r0 as f32);
        let h = |c, r| self.height_at_cell(c, r);
        let top = h(c0, r0) * (1.0 - tx) + h(c1, r0) * tx;
        let bottom = h(c0, r1) * (1.0 - tx) + h(c1, r1) * tx;
        top * (1.0 - ty) + bottom * ty
    }

    pub fn is_water(&self, col: usize, row: usize) -> bool {
        self.height_at_cell(col, row) < self.sea_level
    }

    /// Press the ground flat at `height` inside `radius` of `center`,
    /// blending out over another radius beyond it.
    pub fn flatten(&mut self, center: Vec2, radius: f32, height: f32) {
        let blend = radius;
        let (c0, r0) = self.cell_of(center - Vec2::splat(radius + blend));
        let (c1, r1) = self.cell_of(center + Vec2::splat(radius + blend));
        for row in r0..=r1 {
            for col in c0..=c1 {
                let d = self.cell_center(col, row).distance(center);
                let i = self.index(col, row);
                if d <= radius {
                    self.heights[i] = height;
                } else if d < radius + blend {
                    let t = (d - radius) / blend;
                    self.heights[i] = height * (1.0 - t) + self.heights[i] * t;
                }
            }
        }
    }
}

/// Roll a world. Returns an error when the def names a biome or a
/// monument type nothing registered, or the sites cannot be placed.
pub fn roll_world(
    def: &WorldDef,
    seed: u64,
    biomes: &BiomeRegistry,
    monuments: &crate::monument::MonumentRegistry,
) -> Result<World, String> {
    for rule in &def.biome_rules {
        if biomes.def(&rule.biome).is_none() {
            return Err(format!("biome rule names unregistered biome '{}'", rule.biome));
        }
    }
    for (kind, _) in &def.monuments {
        if monuments.def(kind).is_none() {
            return Err(format!("world names unregistered monument type '{kind}'"));
        }
    }
    let cells = (def.size / def.cell).max(2.0) as usize;
    let mut world = World {
        seed,
        def: def.clone(),
        cells,
        heights: vec![0.0; cells * cells],
        sea_level: def.water_level * def.height_scale,
        biome_of_cell: vec![0; cells * cells],
        biomes: def.biome_rules.iter().map(|r| r.biome.clone()).collect(),
        sites: Vec::new(),
        roads: Vec::new(),
    };
    let elevation = Simplex::new((seed & 0xffff_ffff) as u32);
    let moisture = Simplex::new(((seed >> 32) & 0xffff_ffff) as u32);
    let ridges = Simplex::new((seed.rotate_left(16) & 0xffff_ffff) as u32);
    let regions = Simplex::new((seed.rotate_left(48) & 0xffff_ffff) as u32);

    // Height and biome per cell. Base: three octaves of simplex (Red
    // Blob's fBm) raised to `flatness` so low ground is plains and
    // valleys. Mountains: ridged noise (1 - |n|, Musgrave) where the
    // mountain region mask is high. Plateaus: terraced height where
    // the plateau region mask is high. Biomes read the base fraction.
    let mut moist = vec![0.0f32; cells * cells];
    let mut base = vec![0.0f32; cells * cells];
    let smooth = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    for row in 0..cells {
        for col in 0..cells {
            let p = world.cell_center(col, row);
            let (x, y) = (f64::from(p.x), f64::from(p.y));
            let f = 1.0 / f64::from(def.feature_size);
            let e = (elevation.get([x * f, y * f])
                + 0.5 * elevation.get([x * f * 2.0, y * f * 2.0])
                + 0.25 * elevation.get([x * f * 4.0, y * f * 4.0]))
                / 1.75;
            let e = (((e + 1.0) * 0.5) as f32).powf(def.flatness.max(0.1));
            let m = ((moisture.get([x * f * 0.7, y * f * 0.7]) + 1.0) * 0.5) as f32;
            let mut height = e * def.height_scale;

            // Mountain ranges: regions from a slow noise, ridges from a
            // faster one, squared for sharp crests.
            let region = regions.get([x * f * 0.5, y * f * 0.5]) as f32;
            let mountain_mask = smooth((region - 0.15) / 0.35);
            if mountain_mask > 0.0 && def.mountain_height > 0.0 {
                let r1 = 1.0 - (ridges.get([x * f * 2.0, y * f * 2.0]) as f32).abs();
                let r2 = 1.0 - (ridges.get([x * f * 4.0, y * f * 4.0]) as f32).abs();
                let ridge = (r1 * 0.7 + r2 * 0.3).powi(2);
                height += ridge * mountain_mask * def.mountain_height;
            }
            // Plateaus: the opposite regions, terraced.
            let plateau_mask = smooth((-region - 0.15) / 0.35);
            if plateau_mask > 0.0 && def.plateau_step > 0.0 {
                let stepped = (height / def.plateau_step).round() * def.plateau_step;
                height = height * (1.0 - plateau_mask) + stepped * plateau_mask;
            }

            let i = world.index(col, row);
            world.heights[i] = height;
            base[i] = e;
            moist[i] = m;
        }
    }
    // The bunker was built at height zero: shift the whole map so the
    // ground at the bunker is zero, then press the disc around it flat.
    let bunker_height = world.height_at(def.bunker);
    for h in &mut world.heights {
        *h -= bunker_height;
    }
    world.sea_level -= bunker_height;
    world.flatten(def.bunker, def.bunker_clear, 0.0);
    for row in 0..cells {
        for col in 0..cells {
            let i = world.index(col, row);
            let h = base[i];
            let m = moist[i];
            let rule = def
                .biome_rules
                .iter()
                .position(|r| {
                    h >= r.height.0 && h <= r.height.1 && m >= r.moisture.0 && m <= r.moisture.1
                })
                .unwrap_or(def.biome_rules.len() - 1);
            world.biome_of_cell[i] = rule as u16;
        }
    }

    // Sites by rejection sampling (Endless): random points, kept when
    // on land, spaced from every other site and the bunker, and in a
    // biome that allows the type.
    let mut roll = Roll::new(seed ^ 0x5173);
    let half = def.size / 2.0;
    for (kind, count) in &def.monuments {
        let spacing = monuments.def(kind).map_or(0.0, |d| d.spacing);
        let margin = spacing / 2.0;
        let mut placed = 0;
        let mut attempts = 0;
        while placed < *count && attempts < 4000 {
            attempts += 1;
            let p = Vec2::new(
                roll.measure(-half + margin, half - margin),
                roll.measure(-half + margin, half - margin),
            );
            let (col, row) = world.cell_of(p);
            if world.is_water(col, row) {
                continue;
            }
            if p.distance(def.bunker) < spacing.max(def.bunker_clear)
                || world
                    .sites
                    .iter()
                    .any(|s| s.position.distance(p) < spacing.min(s.spacing))
            {
                continue;
            }
            let biome = world.biome_at_cell(col, row).to_string();
            let allowed = biomes
                .def(&biome)
                .is_some_and(|b| b.monuments.iter().any(|m| m == kind));
            if !allowed {
                continue;
            }
            let position = world.cell_center(col, row);
            // Monuments stand on flat ground.
            let height = world.height_at_cell(col, row);
            world.flatten(position, spacing * 0.25, height);
            world.sites.push(Site {
                monument: kind.clone(),
                position,
                biome,
                spacing,
            });
            placed += 1;
        }
        if placed < *count {
            return Err(format!(
                "could only place {placed} of {count} '{kind}' sites with spacing {spacing}"
            ));
        }
    }

    // Roads: from every site with buildings to the nearest
    // already-connected point (the bunker first), so the network is
    // a tree everyone can walk. Minor sites sit off the road.
    let mut connected: Vec<Vec2> = vec![def.bunker];
    let mut order: Vec<usize> = (0..world.sites.len())
        .filter(|i| {
            monuments
                .def(&world.sites[*i].monument)
                .is_some_and(|d| !d.slots.is_empty())
        })
        .collect();
    order.sort_by(|a, b| {
        let da = world.sites[*a].position.distance(def.bunker);
        let db = world.sites[*b].position.distance(def.bunker);
        da.total_cmp(&db)
    });
    for i in order {
        let from = world.sites[i].position;
        let to = *connected
            .iter()
            .min_by(|a, b| a.distance(from).total_cmp(&b.distance(from)))
            .expect("the bunker is always connected");
        let points = path(&world, from, to)?;
        world.roads.push(Road { site: i, points });
        connected.push(from);
    }
    Ok(world)
}

/// A* over cells from `from` to `to`, cost by distance plus slope
/// (Rust's roads follow the easy ground), never through water.
fn path(world: &World, from: Vec2, to: Vec2) -> Result<Vec<Vec2>, String> {
    #[derive(PartialEq)]
    struct Open(f32, usize);
    impl Eq for Open {}
    impl PartialOrd for Open {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Open {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            other.0.total_cmp(&self.0)
        }
    }
    let n = world.cells;
    let start = {
        let (c, r) = world.cell_of(from);
        r * n + c
    };
    let goal = {
        let (c, r) = world.cell_of(to);
        r * n + c
    };
    let center = |i: usize| world.cell_center(i % n, i / n);
    let mut best: HashMap<usize, f32> = HashMap::new();
    let mut came: HashMap<usize, usize> = HashMap::new();
    let mut open = BinaryHeap::new();
    best.insert(start, 0.0);
    open.push(Open(center(start).distance(center(goal)), start));
    const SLOPE_COST: f32 = 8.0;
    while let Some(Open(_, i)) = open.pop() {
        if i == goal {
            let mut points = vec![center(i)];
            let mut at = i;
            while let Some(&p) = came.get(&at) {
                points.push(center(p));
                at = p;
            }
            points.reverse();
            return Ok(points);
        }
        let g = best[&i];
        let (c, r) = (i % n, i / n);
        for (dc, dr) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            let (nc, nr) = (c as isize + dc, r as isize + dr);
            if nc < 0 || nr < 0 || nc >= n as isize || nr >= n as isize {
                continue;
            }
            let (nc, nr) = (nc as usize, nr as usize);
            if world.is_water(nc, nr) {
                continue;
            }
            let j = nr * n + nc;
            let step = world.def.cell * if dc != 0 && dr != 0 { 1.414 } else { 1.0 };
            let rise = (world.height_at_cell(nc, nr) - world.height_at_cell(c, r)).abs();
            let cost = g + step + rise * SLOPE_COST;
            if best.get(&j).is_none_or(|&b| cost < b) {
                best.insert(j, cost);
                came.insert(j, i);
                open.push(Open(cost + center(j).distance(center(goal)), j));
            }
        }
    }
    Err("no road: the site is cut off by water".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biome::{BiomeDef, BiomeRegistry};
    use crate::monument::{
        Arrangement, BuildingRegistry, BuildingSize, BuildingSlot, BuildingTypeDef, MonumentRegistry,
        MonumentTypeDef, PropSpec,
    };
    use glam::Vec3;

    fn registries() -> (BiomeRegistry, MonumentRegistry) {
        let mut biomes = BiomeRegistry::default();
        for (name, allowed) in [
            ("lowland", vec!["roadside stop", "wreck"]),
            ("hills", vec!["roadside stop", "radio tower", "wreck"]),
        ] {
            biomes
                .register(BiomeDef {
                    name: name.to_string(),
                    ground: [0.3, 0.4, 0.2],
                    scatter: vec![],
                    weather: vec![],
                    monuments: allowed.into_iter().map(String::from).collect(),
                    npcs: vec![],
                    wildlife: vec![],
                    harvest: vec![],
                })
                .unwrap();
        }
        let mut buildings = BuildingRegistry::default();
        buildings
            .register(BuildingTypeDef {
                name: "shed".to_string(),
                size: BuildingSize::Small,
                columns: (1, 1),
                rows: (1, 1),
                floors: (1, 1),
                basements: (0, 0),
                width: (4.0, 5.0),
                length: (4.0, 5.0),
                height: (2.6, 3.0),
                windows: 0,
                clutter: 0,
                lights: 0,
                damage: 0,
                carve: 0,
                palette: vec![[0.5, 0.5, 0.5]],
            })
            .unwrap();
        let mut monuments = MonumentRegistry::default();
        for name in ["roadside stop", "radio tower"] {
            monuments
                .register(
                    MonumentTypeDef {
                        name: name.to_string(),
                        slots: vec![BuildingSlot {
                            choices: vec!["shed".to_string()],
                            min: 1,
                            max: 1,
                        }],
                        arrangement: Arrangement::AlongRoad,
                        danger: 1,
                        gated: false,
                        suffix: "stop".to_string(),
                        spacing: 120.0,
                        props: vec![],
                    },
                    &buildings,
                )
                .unwrap();
        }
        monuments
            .register(
                MonumentTypeDef {
                    name: "wreck".to_string(),
                    slots: vec![],
                    arrangement: Arrangement::Clustered,
                    danger: 0,
                    gated: false,
                    suffix: "wreck".to_string(),
                    spacing: 40.0,
                    props: vec![PropSpec {
                        size: Vec3::new(4.0, 1.4, 1.8),
                        color: [0.4, 0.2, 0.15],
                        count: (1, 2),
                        radius: 4.0,
                    }],
                },
                &buildings,
            )
            .unwrap();
        (biomes, monuments)
    }

    fn def() -> WorldDef {
        WorldDef {
            size: 800.0,
            cell: 8.0,
            height_scale: 30.0,
            feature_size: 250.0,
            flatness: 1.6,
            mountain_height: 40.0,
            plateau_step: 6.0,
            water_level: 0.2,
            biome_rules: vec![
                BiomeRule {
                    biome: "hills".to_string(),
                    height: (0.55, 1.0),
                    moisture: (0.0, 1.0),
                },
                BiomeRule {
                    biome: "lowland".to_string(),
                    height: (0.0, 1.0),
                    moisture: (0.0, 1.0),
                },
            ],
            monuments: vec![
                ("roadside stop".to_string(), 4),
                ("radio tower".to_string(), 1),
                ("wreck".to_string(), 30),
            ],
            bunker: Vec2::ZERO,
            bunker_clear: 20.0,
        }
    }

    #[test]
    fn the_same_seed_rolls_the_same_world_and_another_differs() {
        let (biomes, monuments) = registries();
        let a = roll_world(&def(), 7, &biomes, &monuments).unwrap();
        let b = roll_world(&def(), 7, &biomes, &monuments).unwrap();
        assert_eq!(a, b);
        let c = roll_world(&def(), 8, &biomes, &monuments).unwrap();
        assert_ne!(a.heights, c.heights);
        assert_eq!(a.cells, 100);
        assert_eq!(a.heights.len(), 100 * 100);
    }

    #[test]
    fn sites_are_spaced_on_land_in_an_allowed_biome_and_every_one_has_a_road() {
        let (biomes, monuments) = registries();
        for seed in 1..=12u64 {
            let world = roll_world(&def(), seed, &biomes, &monuments).unwrap();
            assert_eq!(world.sites.len(), 35, "seed {seed}");
            for (i, site) in world.sites.iter().enumerate() {
                let (c, r) = world.cell_of(site.position);
                assert!(!world.is_water(c, r), "seed {seed}: site on water");
                assert!(
                    site.position.distance(Vec2::ZERO) >= site.spacing,
                    "seed {seed}: on the bunker"
                );
                // Two sites keep the smaller of their spacings.
                for other in &world.sites[i + 1..] {
                    let least = site.spacing.min(other.spacing);
                    assert!(
                        other.position.distance(site.position) >= least,
                        "seed {seed}: crowded: {} and {}",
                        site.monument,
                        other.monument
                    );
                }
                let allowed = biomes.def(&site.biome).unwrap();
                assert!(allowed.monuments.contains(&site.monument), "seed {seed}: {site:?}");
                if site.monument == "radio tower" {
                    assert_eq!(site.biome, "hills");
                }
            }
            assert_eq!(world.roads.len(), 5, "one road per built site, none to wrecks");
            for road in &world.roads {
                assert!(road.points.len() >= 2);
                for p in &road.points {
                    let (c, r) = world.cell_of(*p);
                    assert!(!world.is_water(c, r), "seed {seed}: road through water");
                }
            }
            // Every road starts at its site and ends on the bunker or
            // on another site.
            for road in &world.roads {
                let site = &world.sites[road.site];
                let first = road.points[0];
                let last = *road.points.last().unwrap();
                let (sc, sr) = world.cell_of(site.position);
                assert_eq!(world.cell_of(first), (sc, sr), "seed {seed}: road starts at its site");
                let ends_well = world.cell_of(last) == world.cell_of(Vec2::ZERO)
                    || world
                        .sites
                        .iter()
                        .any(|s| world.cell_of(s.position) == world.cell_of(last));
                assert!(ends_well, "seed {seed}: road ends nowhere");
            }
        }
    }

    /// Bethesda's rule: from any point on land, something worth
    /// stopping for within 30 seconds of walking.
    #[test]
    fn something_worth_stopping_for_within_reach_of_most_land() {
        let (biomes, monuments) = registries();
        let world = roll_world(&def(), 4, &biomes, &monuments).unwrap();
        let reach = 150.0;
        let mut roll = Roll::new(99);
        let (mut land, mut covered) = (0, 0);
        for _ in 0..200 {
            let p = Vec2::new(roll.measure(-380.0, 380.0), roll.measure(-380.0, 380.0));
            let (c, r) = world.cell_of(p);
            if world.is_water(c, r) {
                continue;
            }
            land += 1;
            if world.sites.iter().any(|s| s.position.distance(p) <= reach) {
                covered += 1;
            }
        }
        assert!(land > 100, "the test world is mostly land");
        assert!(
            covered * 10 >= land * 9,
            "{covered} of {land} land points have a site within {reach} m"
        );
    }

    #[test]
    fn the_bunker_and_every_site_stand_on_flat_ground() {
        let (biomes, monuments) = registries();
        let world = roll_world(&def(), 3, &biomes, &monuments).unwrap();
        // Inside the flat radius less one cell (the edge cells blend).
        let at = world.height_at(Vec2::ZERO);
        assert!(at.abs() < 1e-4, "the bunker stands at height zero, not {at}");
        for d in [Vec2::new(10.0, 0.0), Vec2::new(-6.0, 8.0), Vec2::new(0.0, -10.0)] {
            assert!((world.height_at(d) - at).abs() < 1e-3, "bunker ground slopes at {d}");
        }
        for site in &world.sites {
            let h = world.height_at(site.position);
            // Flat to a quarter of the spacing, less one cell of blend.
            let out = site.spacing * 0.25 - world.def.cell * 1.5;
            let at = world.height_at(site.position + Vec2::new(out, 0.0));
            assert!((at - h).abs() < 1e-3, "{} slopes {out} m out: {at} vs {h}", site.monument);
        }
    }

    #[test]
    fn mountains_and_plateaus_shape_the_base_terrain() {
        let (biomes, monuments) = registries();
        let mut plain = def();
        plain.mountain_height = 0.0;
        plain.plateau_step = 0.0;
        let flat = roll_world(&plain, 5, &biomes, &monuments).unwrap();
        let shaped = roll_world(&def(), 5, &biomes, &monuments).unwrap();
        let relief = |w: &World| {
            let max = w.heights.iter().cloned().fold(f32::MIN, f32::max);
            let min = w.heights.iter().cloned().fold(f32::MAX, f32::min);
            max - min
        };
        assert!(relief(&shaped) > relief(&flat) + 10.0, "mountains add relief");
        // Plateaus: terraces make runs of cells at exactly the same
        // height; smooth noise never does outside the flattened discs.
        let level_runs = |w: &World| {
            let mut n = 0;
            for row in 0..w.cells {
                for col in 0..w.cells - 1 {
                    if w.height_at_cell(col, row) == w.height_at_cell(col + 1, row) {
                        n += 1;
                    }
                }
            }
            n
        };
        assert!(
            level_runs(&shaped) > level_runs(&flat) + 200,
            "terraced runs: {} vs {}",
            level_runs(&shaped),
            level_runs(&flat)
        );
        // The biome picture is the same either way: the rules read the
        // base fraction, not the mountains and terraces.
        assert!(flat.biome_of_cell == shaped.biome_of_cell, "biomes follow the base terrain");
    }

    #[test]
    fn unregistered_names_are_refused() {
        let (biomes, monuments) = registries();
        let mut bad = def();
        bad.monuments.push(("airport".to_string(), 1));
        assert!(roll_world(&bad, 1, &biomes, &monuments).is_err());
        let mut bad = def();
        bad.biome_rules[0].biome = "swamp".to_string();
        assert!(roll_world(&bad, 1, &biomes, &monuments).is_err());
    }
}
