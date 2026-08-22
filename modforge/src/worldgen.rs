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

use glam::{Vec2, Vec3};
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

/// Which generator rolls the ground (design.md "The generators we
/// want"): each reality is one of these, and every one ends in the
/// same `World` data.
#[derive(Clone, Debug, PartialEq)]
pub enum Generator {
    /// The mixed world, Rust's map: flat lowlands, ridged mountain
    /// ranges in some regions, stepped plateaus in others. `flatness`
    /// is the power the base noise is raised to (1 is plain fBm; 2
    /// flattens the low ground into plains and valleys).
    /// `mountain_height` is the extra metres ridged mountains add
    /// where the mountain mask is high; `plateau_step` is the terrace
    /// height where the plateau mask is high (0 for none).
    Relief {
        flatness: f32,
        mountain_height: f32,
        plateau_step: f32,
    },
    /// The Labyrinth (design.md): a maze of paths and dead ends on
    /// flat ground, walls as real structures with no ceiling. Maze
    /// cells are `corridor` metres square and the walls are as high
    /// as the corridor is wide (operator rule, 2026-08-21); the
    /// bunker sits in a clearing of `bunker_clear`. Carved by
    /// recursive backtracker (Jamis Buck, "Mazes for Programmers").
    Labyrinth { corridor: f32, wall_thickness: f32 },
}

/// One straight run of wall, axis-aligned, from `start` to `end` in
/// world metres (x east, y the world's z), standing on the ground.
/// The consumer builds it as a structure.
#[derive(Clone, Debug, PartialEq)]
pub struct Wall {
    pub start: Vec2,
    pub end: Vec2,
    pub height: f32,
    pub thickness: f32,
}

impl Wall {
    /// The ground the wall stands on, as a min and max corner: the
    /// run's extent along its axis, half a thickness across.
    pub fn footprint(&self) -> (Vec2, Vec2) {
        let lo = self.start.min(self.end);
        let hi = self.start.max(self.end);
        let t = self.thickness / 2.0;
        if self.start.y == self.end.y {
            (Vec2::new(lo.x, lo.y - t), Vec2::new(hi.x, hi.y + t))
        } else {
            (Vec2::new(lo.x - t, lo.y), Vec2::new(hi.x + t, hi.y))
        }
    }

    /// Distance from `p` to the wall's centre line.
    pub fn distance(&self, p: Vec2) -> f32 {
        let d = self.end - self.start;
        let len2 = d.length_squared();
        let t = if len2 == 0.0 { 0.0 } else { ((p - self.start).dot(d) / len2).clamp(0.0, 1.0) };
        (self.start + d * t).distance(p)
    }
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
    pub generator: Generator,
    /// Below this fraction of height_scale the cell is water.
    pub water_level: f32,
    pub biome_rules: Vec<BiomeRule>,
    /// Monument types to place, as (type name, how many). Each type's
    /// own `spacing` keeps it off other sites and the bunker: two
    /// sites may stand as close as the smaller of their spacings, so
    /// a wreck can sit near a city but two cities never touch.
    pub monuments: Vec<(String, u32)>,
    /// How many sites of the tallest type in `monuments` must be in
    /// sight from the bunker's door (topside design.md "The triangle
    /// rule": the door opens on one big triangle). Placed before
    /// everything else.
    pub landmarks: u32,
    /// Where the bunker is; the world rolls around it. Flat ground is
    /// kept in `bunker_clear` around it. No road leads to it: roads
    /// join sites to each other, the bunker is found, not followed.
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
    /// Walls the generator built (the Labyrinth); empty for the
    /// mixed world.
    pub walls: Vec<Wall>,
}

impl World {
    /// True when `p` is clear of every wall: a site or a road may
    /// stand here.
    pub fn in_corridor(&self, p: Vec2) -> bool {
        self.walls.iter().all(|w| w.distance(p) > w.thickness / 2.0)
    }

    /// Whether a point `to` (x, height, y) is in sight from `from`
    /// over the ground and the walls: the line between them stays
    /// above the heightmap at every cell and above every wall it
    /// crosses. The triangle rule's one check: landmarks must pass
    /// it, and a site behind a hill must fail it.
    pub fn sees(&self, from: Vec3, to: Vec3) -> bool {
        let a = Vec2::new(from.x, from.z);
        let b = Vec2::new(to.x, to.z);
        let flat = a.distance(b);
        let steps = ((flat / self.def.cell).ceil() as usize).max(1);
        for i in 1..steps {
            let t = i as f32 / steps as f32;
            let p = a.lerp(b, t);
            let h = from.y + (to.y - from.y) * t;
            if self.height_at(p) >= h {
                return false;
            }
        }
        for w in &self.walls {
            let top = self.height_at((w.start + w.end) / 2.0) + w.height;
            if let Some(t) = crossing(a, b, w.start, w.end)
                && from.y + (to.y - from.y) * t < top
            {
                return false;
            }
        }
        true
    }

    /// Per heightmap cell, whether a wall's footprint overlaps it:
    /// such a cell holds no site, no road, no scatter. Marked per
    /// wall over the cells its footprint covers, so the cost is the
    /// walls' area, not cells times walls.
    pub fn blocked_cells(&self) -> Vec<bool> {
        let mut blocked = vec![false; self.cells * self.cells];
        let half = self.def.cell / 2.0;
        for w in &self.walls {
            let (min, max) = w.footprint();
            let (c0, r0) = self.cell_of(min);
            let (c1, r1) = self.cell_of(max);
            for row in r0..=r1 {
                for col in c0..=c1 {
                    let p = self.cell_center(col, row);
                    // Overlap, touching edges excluded.
                    if p.x - half < max.x && p.x + half > min.x && p.y - half < max.y && p.y + half > min.y {
                        blocked[self.index(col, row)] = true;
                    }
                }
            }
        }
        blocked
    }

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
        walls: Vec::new(),
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
    // The Labyrinth keeps the ground flat and the biome picture.
    let (flatness, mountain_height, plateau_step) = match def.generator {
        Generator::Relief {
            flatness,
            mountain_height,
            plateau_step,
        } => (flatness, mountain_height, plateau_step),
        Generator::Labyrinth { .. } => (1.0, 0.0, 0.0),
    };
    let flat = matches!(def.generator, Generator::Labyrinth { .. });
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
            let raw = ((e + 1.0) * 0.5) as f32;
            let e = raw.powf(flatness.max(0.1));
            let m = ((moisture.get([x * f * 0.7, y * f * 0.7]) + 1.0) * 0.5) as f32;
            let mut height = e * def.height_scale;

            // Mountain ranges: regions from a slow noise, ridges from a
            // faster one, squared for sharp crests.
            let region = regions.get([x * f * 0.5, y * f * 0.5]) as f32;
            let mountain_mask = smooth((region - 0.15) / 0.35);
            if mountain_mask > 0.0 && mountain_height > 0.0 {
                let r1 = 1.0 - (ridges.get([x * f * 2.0, y * f * 2.0]) as f32).abs();
                let r2 = 1.0 - (ridges.get([x * f * 4.0, y * f * 4.0]) as f32).abs();
                let ridge = (r1 * 0.7 + r2 * 0.3).powi(2);
                height += ridge * mountain_mask * mountain_height;
            }
            // Plateaus: the opposite regions, terraced.
            let plateau_mask = smooth((-region - 0.15) / 0.35);
            if plateau_mask > 0.0 && plateau_step > 0.0 {
                let stepped = (height / plateau_step).round() * plateau_step;
                height = height * (1.0 - plateau_mask) + stepped * plateau_mask;
            }

            let i = world.index(col, row);
            world.heights[i] = if flat { 0.0 } else { height };
            // Biome rules read the raw fraction, so the biome picture
            // does not move when the relief numbers change.
            base[i] = raw;
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

    // The Labyrinth's walls, and which heightmap cells they block.
    if let Generator::Labyrinth {
        corridor,
        wall_thickness,
    } = def.generator
    {
        let wall_height = corridor;
        let n = maze_cells(def, corridor);
        // The bunker's clearing: every maze cell inside `bunker_clear`
        // is open ground the maze grows out of, so the bunker and its
        // house stand in a plaza however narrow the corridors are.
        let origin = maze_origin(def, n, corridor);
        let clearing: Vec<bool> = (0..n * n)
            .map(|i| {
                let c = origin + Vec2::new((i % n) as f32 + 0.5, (i / n) as f32 + 0.5) * corridor;
                c.distance(def.bunker) < def.bunker_clear
            })
            .collect();
        let open = carve_maze(n, &clearing, &mut Roll::new(seed ^ 0x4d41_5a45));
        world.walls = walls_of_maze(&open, n, origin, def, corridor, wall_height, wall_thickness);
    }
    // Cells no site or road may use: a wall stands there, or it is
    // the bunker's clearing (no road leads to the bunker).
    let mut blocked = world.blocked_cells();
    for (i, b) in blocked.iter_mut().enumerate() {
        if world.cell_center(i % cells, i / cells).distance(def.bunker) < def.bunker_clear {
            *b = true;
        }
    }
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
    // The landmarks: the tallest type in the list, placed first, and
    // the first `landmarks` of them in sight from the bunker's door
    // (eye height, 1.7 m). Every other type follows in list order.
    let landmark_type = def
        .monuments
        .iter()
        .filter_map(|(kind, _)| monuments.def(kind))
        .max_by(|a, b| a.height.total_cmp(&b.height))
        .map(|d| d.name.clone());
    let door = Vec3::new(def.bunker.x, 1.7, def.bunker.y);
    let mut order: Vec<&(String, u32)> = def.monuments.iter().collect();
    order.sort_by_key(|(kind, _)| Some(kind) != landmark_type.as_ref());
    for (kind, count) in order {
        let monument = monuments.def(kind).expect("checked above");
        let spacing = monument.spacing;
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
            // Spacing is checked on the snapped centre, where the site
            // will actually stand. A site stands in a corridor, never
            // in a wall.
            let p = world.cell_center(col, row);
            if blocked[world.index(col, row)] {
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
            let height = world.height_at_cell(col, row);
            // A landmark stands where its top is in sight from the
            // door.
            let landmark = Some(kind) == landmark_type.as_ref() && placed < def.landmarks;
            if landmark
                && !world.sees(door, Vec3::new(position.x, height + monument.height, position.y))
            {
                continue;
            }
            // Monuments stand on flat ground.
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

    // Roads: from every site with buildings to the nearest site
    // already on the network, so the roads are a tree between the
    // places people used. None leads to the bunker (the door opens
    // on a landmark, not a road); the nearest site to the bunker is
    // the tree's root and has no road of its own. Minor sites sit
    // off the road.
    let mut connected: Vec<Vec2> = Vec::new();
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
        if let Some(to) = connected
            .iter()
            .min_by(|a, b| a.distance(from).total_cmp(&b.distance(from)))
            .copied()
        {
            let points = path(&world, &blocked, from, to)?;
            world.roads.push(Road { site: i, points });
        }
        connected.push(from);
    }
    Ok(world)
}

/// A* over cells from `from` to `to`, cost by distance plus slope
/// (Rust's roads follow the easy ground), never through water or a
/// `blocked` cell (a wall).
fn path(world: &World, blocked: &[bool], from: Vec2, to: Vec2) -> Result<Vec<Vec2>, String> {
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
            let j = nr * n + nc;
            if world.is_water(nc, nr) || (blocked[j] && j != goal) {
                continue;
            }
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
    Err("no road: the site is cut off by water or walls".to_string())
}

/// Carve a maze of `n` by `n` cells by recursive backtracker: walk
/// to a random unvisited neighbour, knocking the wall between, back
/// up when stuck. Every cell is reached, there is one path between
/// any two cells, and the backing up leaves dead ends. Returns the
/// open sides per cell as bits: 1 north (+y), 2 east, 4 south, 8
/// west. `clearing` marks cells that are open ground before the
/// walk (the bunker's plaza): they open to each other and the walk
/// grows out of them. Seeded, so the same seed carves the same maze.
pub fn carve_maze(n: usize, clearing: &[bool], roll: &mut Roll) -> Vec<u8> {
    const N: u8 = 1;
    const E: u8 = 2;
    const S: u8 = 4;
    const W: u8 = 8;
    let mut open = vec![0u8; n * n];
    let mut seen = vec![false; n * n];
    let start = (n / 2) * n + n / 2;
    let mut stack = vec![start];
    seen[start] = true;
    for i in 0..n * n {
        if !clearing.get(i).copied().unwrap_or(false) {
            continue;
        }
        seen[i] = true;
        stack.push(i);
        let (x, y) = (i % n, i / n);
        if y + 1 < n && clearing[i + n] {
            open[i] |= N;
            open[i + n] |= S;
        }
        if x + 1 < n && clearing[i + 1] {
            open[i] |= E;
            open[i + 1] |= W;
        }
    }
    while let Some(&at) = stack.last() {
        let (x, y) = (at % n, at / n);
        let mut next = [(0usize, 0u8, 0u8); 4];
        let mut count = 0;
        if y + 1 < n && !seen[at + n] {
            next[count] = (at + n, N, S);
            count += 1;
        }
        if x + 1 < n && !seen[at + 1] {
            next[count] = (at + 1, E, W);
            count += 1;
        }
        if y > 0 && !seen[at - n] {
            next[count] = (at - n, S, N);
            count += 1;
        }
        if x > 0 && !seen[at - 1] {
            next[count] = (at - 1, W, E);
            count += 1;
        }
        if count == 0 {
            stack.pop();
            continue;
        }
        let (to, side, back) = *roll.pick(&next[..count]);
        open[at] |= side;
        open[to] |= back;
        seen[to] = true;
        stack.push(to);
    }
    open
}

/// Where segment a-b crosses segment c-d, as the fraction along a-b,
/// or None when they do not cross.
fn crossing(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<f32> {
    let r = b - a;
    let s = d - c;
    let denom = r.perp_dot(s);
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = (c - a).perp_dot(s) / denom;
    let u = (c - a).perp_dot(r) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

/// Maze cells per side: an odd count, the bunker's cell in the
/// middle, and the whole grid inside the world. A grid that poked
/// past the edge got its outer ring clipped, and the walk threads
/// through that ring, so clipping cut the maze into pieces.
fn maze_cells(def: &WorldDef, corridor: f32) -> usize {
    let room = def.size / 2.0 - def.bunker.x.abs().max(def.bunker.y.abs()) - corridor / 2.0;
    let each_side = (room / corridor).floor().max(1.0) as usize;
    2 * each_side + 1
}

/// Where the maze grid starts: laid so the bunker sits at the centre
/// of cell (n/2, n/2).
fn maze_origin(def: &WorldDef, n: usize, corridor: f32) -> Vec2 {
    def.bunker - Vec2::splat(corridor / 2.0 + (n / 2) as f32 * corridor)
}

/// The walls a carved maze needs: every closed side becomes wall,
/// collinear pieces merged into one run, plus the outer boundary.
/// Runs are clipped to the world.
fn walls_of_maze(
    open: &[u8],
    n: usize,
    origin: Vec2,
    def: &WorldDef,
    corridor: f32,
    height: f32,
    thickness: f32,
) -> Vec<Wall> {
    let half = def.size / 2.0;
    let clip = |v: f32| v.clamp(-half, half);
    let mut walls = Vec::new();
    let mut push = |a: Vec2, b: Vec2| {
        let (a, b) = (Vec2::new(clip(a.x), clip(a.y)), Vec2::new(clip(b.x), clip(b.y)));
        if a.distance(b) > 0.01 {
            walls.push(Wall {
                start: a,
                end: b,
                height,
                thickness,
            });
        }
    };
    // Horizontal runs: along the north side of each row (y = origin.y
    // + (row + 1) * corridor) and the south side of row 0.
    for row in 0..=n {
        let y = origin.y + row as f32 * corridor;
        let mut run: Option<(usize, usize)> = None;
        for col in 0..=n {
            // Closed when the cell below has no north opening (or row 0,
            // the boundary); the cell above's south bit mirrors it.
            let closed = col < n
                && (row == 0 || row == n || open[(row - 1) * n + col] & 1 == 0);
            match (closed, run) {
                (true, None) => run = Some((col, col)),
                (true, Some((s, _))) => run = Some((s, col)),
                (false, Some((s, e))) => {
                    push(
                        Vec2::new(origin.x + s as f32 * corridor - thickness / 2.0, y),
                        Vec2::new(origin.x + (e + 1) as f32 * corridor + thickness / 2.0, y),
                    );
                    run = None;
                }
                (false, None) => {}
            }
        }
    }
    // Vertical runs: along the east side of each column and the west
    // side of column 0.
    for col in 0..=n {
        let x = origin.x + col as f32 * corridor;
        let mut run: Option<(usize, usize)> = None;
        for row in 0..=n {
            let closed = row < n
                && (col == 0 || col == n || open[row * n + col - 1] & 2 == 0);
            match (closed, run) {
                (true, None) => run = Some((row, row)),
                (true, Some((s, _))) => run = Some((s, row)),
                (false, Some((s, e))) => {
                    push(
                        Vec2::new(x, origin.y + s as f32 * corridor - thickness / 2.0),
                        Vec2::new(x, origin.y + (e + 1) as f32 * corridor + thickness / 2.0),
                    );
                    run = None;
                }
                (false, None) => {}
            }
        }
    }
    walls
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
                        good_for: Default::default(),
                        // The radio tower is the landmark.
                        height: if name == "radio tower" { 40.0 } else { 3.0 },
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
                    good_for: Default::default(),
                    height: 1.4,
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
            generator: Generator::Relief {
                flatness: 1.6,
                mountain_height: 40.0,
                plateau_step: 6.0,
            },
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
            landmarks: 1,
            bunker: Vec2::ZERO,
            bunker_clear: 20.0,
        }
    }

    /// The triangle rule: the radio tower (the tallest type) stands
    /// where its top is in sight from the bunker's door; a wreck
    /// behind a hill is not.
    #[test]
    fn the_landmark_is_in_sight_from_the_door() {
        let (biomes, monuments) = registries();
        for seed in 1..=8u64 {
            let world = roll_world(&def(), seed, &biomes, &monuments).unwrap();
            let door = Vec3::new(0.0, 1.7, 0.0);
            let tower = world.sites.iter().find(|s| s.monument == "radio tower").unwrap();
            let ground = world.height_at(tower.position);
            let top = Vec3::new(tower.position.x, ground + 40.0, tower.position.y);
            assert!(world.sees(door, top), "seed {seed}: the tower is hidden");
            // Its foot need not be: it may stand behind a hill.
            assert!(
                world.sites.iter().any(|s| {
                    let h = world.height_at(s.position);
                    !world.sees(door, Vec3::new(s.position.x, h + 1.4, s.position.y))
                }),
                "seed {seed}: something is hidden from the door"
            );
        }
    }

    #[test]
    fn sight_stops_at_the_ground_and_at_a_wall() {
        let (biomes, monuments) = registries();
        let world = roll_world(&labyrinth_def(), 2, &biomes, &monuments).unwrap();
        // Across the clearing: clear. Across a wall: blocked. Over the
        // wall: clear again.
        let eye = Vec3::new(0.0, 1.7, 0.0);
        assert!(world.sees(eye, Vec3::new(8.0, 1.7, 0.0)));
        let w = &world.walls[0];
        let mid = (w.start + w.end) / 2.0;
        let across = if w.start.y == w.end.y { Vec2::new(0.0, 3.0) } else { Vec2::new(3.0, 0.0) };
        let (a, b) = (mid - across, mid + across);
        assert!(!world.sees(Vec3::new(a.x, 1.7, a.y), Vec3::new(b.x, 1.7, b.y)), "a wall blocks");
        assert!(world.sees(Vec3::new(a.x, 12.0, a.y), Vec3::new(b.x, 12.0, b.y)), "over the wall");
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
            assert_eq!(
                world.roads.len(),
                4,
                "one road per built site but the root, none to wrecks, none to the bunker"
            );
            for road in &world.roads {
                assert!(road.points.len() >= 2);
                for p in &road.points {
                    let (c, r) = world.cell_of(*p);
                    assert!(!world.is_water(c, r), "seed {seed}: road through water");
                }
            }
            // Every road starts at its site and ends on another site,
            // never at the bunker, and none comes near it.
            for road in &world.roads {
                let site = &world.sites[road.site];
                let first = road.points[0];
                let last = *road.points.last().unwrap();
                let (sc, sr) = world.cell_of(site.position);
                assert_eq!(world.cell_of(first), (sc, sr), "seed {seed}: road starts at its site");
                let ends_well = world
                    .sites
                    .iter()
                    .any(|s| world.cell_of(s.position) == world.cell_of(last));
                assert!(ends_well, "seed {seed}: road ends nowhere");
                for p in &road.points {
                    assert!(p.distance(Vec2::ZERO) > 20.0, "seed {seed}: a road at the bunker");
                }
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
        plain.generator = Generator::Relief {
            flatness: 1.0,
            mountain_height: 0.0,
            plateau_step: 0.0,
        };
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

    /// The mixed world must not move when the roll is refactored: a
    /// fingerprint of seed 7's heights, sites, and roads, recorded
    /// before `Generator` was added.
    #[test]
    fn the_mixed_world_rolls_as_it_always_has() {
        let (biomes, monuments) = registries();
        let world = roll_world(&def(), 7, &biomes, &monuments).unwrap();
        let mut sum = 0u64;
        for h in &world.heights {
            sum = sum.wrapping_mul(31).wrapping_add(h.to_bits() as u64);
        }
        for s in &world.sites {
            sum = sum.wrapping_mul(31).wrapping_add(s.position.x.to_bits() as u64);
            sum = sum.wrapping_mul(31).wrapping_add(s.position.y.to_bits() as u64);
        }
        for r in &world.roads {
            sum = sum.wrapping_mul(31).wrapping_add(r.points.len() as u64);
        }
        assert_eq!(sum, FINGERPRINT, "the mixed world changed");
    }

    // Re-recorded 2026-08-21 when landmarks began placing first and
    // roads stopped leading to the bunker (sites and roads moved; the
    // heights did not).
    const FINGERPRINT: u64 = 871338364202514350;

    fn labyrinth_def() -> WorldDef {
        let mut d = def();
        d.generator = Generator::Labyrinth {
            corridor: 10.0,
            wall_thickness: 1.0,
        };
        // Cells must be finer than the corridor for sites and roads
        // to find the free band down its middle.
        d.cell = 4.0;
        d.water_level = 0.0;
        d.monuments = vec![("roadside stop".to_string(), 2), ("wreck".to_string(), 20)];
        // Nothing in the maze stands over its walls yet, so no
        // landmark can be in sight from the door.
        d.landmarks = 0;
        d
    }

    #[test]
    fn the_maze_reaches_every_cell_from_the_bunker_and_has_dead_ends() {
        let n = 20;
        let none = vec![false; n * n];
        let open = carve_maze(n, &none, &mut Roll::new(11));
        // Flood from the bunker's cell over open sides.
        let mut seen = vec![false; n * n];
        let start = (n / 2) * n + n / 2;
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(at) = stack.pop() {
            let (x, y) = (at % n, at / n);
            for (bit, to) in [
                (1u8, if y + 1 < n { Some(at + n) } else { None }),
                (2, if x + 1 < n { Some(at + 1) } else { None }),
                (4, if y > 0 { Some(at - n) } else { None }),
                (8, if x > 0 { Some(at - 1) } else { None }),
            ] {
                if let Some(to) = to
                    && open[at] & bit != 0
                    && !seen[to]
                {
                    seen[to] = true;
                    stack.push(to);
                }
            }
        }
        assert!(seen.iter().all(|s| *s), "every cell reachable");
        let dead_ends = open.iter().filter(|o| o.count_ones() == 1).count();
        assert!(dead_ends > 10, "a maze has dead ends: {dead_ends}");
        assert_eq!(open, carve_maze(n, &none, &mut Roll::new(11)), "same seed same maze");
        assert_ne!(open, carve_maze(n, &none, &mut Roll::new(12)));

        // A clearing in the middle is open ground the maze grows
        // from: its cells open to each other, and the rest is still
        // reached.
        let clearing: Vec<bool> = (0..n * n)
            .map(|i| (i % n).abs_diff(n / 2) <= 1 && (i / n).abs_diff(n / 2) <= 1)
            .collect();
        let open = carve_maze(n, &clearing, &mut Roll::new(11));
        let mid = (n / 2) * n + n / 2;
        assert_eq!(open[mid], 15, "the clearing's centre is open on all sides");
        assert!(open.iter().all(|o| *o != 0), "every cell has an opening");
    }

    /// Every free heightmap cell of the Labyrinth is reachable from
    /// the bunker over free cells: the corridors are one connected
    /// walk, so every road can be found.
    #[test]
    fn the_labyrinths_free_cells_are_one_connected_walk() {
        let (biomes, monuments) = registries();
        let mut def = labyrinth_def();
        def.monuments.clear();
        let world = roll_world(&def, 1, &biomes, &monuments).unwrap();
        let n = world.cells;
        let mut blocked = world.blocked_cells();
        // The strip of ground outside the maze's outer wall is free
        // but not part of the walk: count it as blocked.
        let (lo, hi) = world.walls.iter().fold((Vec2::MAX, Vec2::MIN), |(lo, hi), w| {
            (lo.min(w.start).min(w.end), hi.max(w.start).max(w.end))
        });
        for i in 0..n * n {
            let p = world.cell_center(i % n, i / n);
            if p.x < lo.x || p.x > hi.x || p.y < lo.y || p.y > hi.y {
                blocked[i] = true;
            }
        }
        let free = blocked.iter().filter(|b| !**b).count();
        let (bc, br) = world.cell_of(Vec2::ZERO);
        let mut seen = vec![false; n * n];
        let mut stack = vec![br * n + bc];
        seen[br * n + bc] = true;
        let mut reached = 0;
        while let Some(i) = stack.pop() {
            reached += 1;
            let (c, r) = ((i % n) as isize, (i / n) as isize);
            for dr in -1..=1isize {
                for dc in -1..=1isize {
                    let (nc, nr) = (c + dc, r + dr);
                    if nc < 0 || nr < 0 || nc >= n as isize || nr >= n as isize {
                        continue;
                    }
                    let j = nr as usize * n + nc as usize;
                    if !blocked[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        // A picture of the cells around the bunker when it fails:
        // '#' blocked, '.' reached, 'x' free but cut off.
        if reached != free {
            for r in (br.saturating_sub(14)..(br + 14).min(n)).rev() {
                let line: String = (bc.saturating_sub(20)..(bc + 20).min(n))
                    .map(|c| {
                        let i = r * n + c;
                        if blocked[i] { '#' } else if seen[i] { '.' } else { 'x' }
                    })
                    .collect();
                eprintln!("{line}");
            }
        }
        assert_eq!(reached, free, "free cells cut off from the bunker: {}", free - reached);
    }

    #[test]
    fn the_labyrinth_is_flat_with_walls_and_sites_and_roads_in_corridors() {
        let (biomes, monuments) = registries();
        for seed in 1..=6u64 {
            let world = roll_world(&labyrinth_def(), seed, &biomes, &monuments).unwrap();
            assert!(world.heights.iter().all(|h| *h == 0.0), "seed {seed}: flat");
            assert!(world.walls.len() > 100, "seed {seed}: {} walls", world.walls.len());
            for w in &world.walls {
                assert!(w.start.x == w.end.x || w.start.y == w.end.y, "axis-aligned");
                assert_eq!(w.height, 10.0, "as high as the corridor is wide");
            }
            // The bunker's clearing (20 m) is open ground: no wall
            // inside it, so the bunker and its house fit in a 10 m
            // maze.
            for d in [Vec2::ZERO, Vec2::new(12.0, 0.0), Vec2::new(0.0, -12.0), Vec2::new(-9.0, 9.0)] {
                assert!(world.in_corridor(d), "seed {seed}: a wall in the clearing at {d}");
            }
            assert_eq!(world.sites.len(), 22, "seed {seed}");
            for site in &world.sites {
                assert!(world.in_corridor(site.position), "seed {seed}: site in a wall");
            }
            // A road winds through the corridors from the second
            // built site to the first.
            assert_eq!(world.roads.len(), 1);
            for road in &world.roads {
                for p in &road.points {
                    assert!(world.in_corridor(*p), "seed {seed}: road through a wall at {p}");
                }
            }
        }
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
