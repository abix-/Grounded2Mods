//! Rectangular annex geometry for grid-based settlements.

/// An inclusive rectangle in grid coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub minx: i64,
    pub miny: i64,
    pub maxx: i64,
    pub maxy: i64,
}

/// A side of the existing rectangle where an annex may attach.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    East,
    South,
    West,
    North,
}

impl Side {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::East => "east",
            Self::South => "south",
            Self::West => "west",
            Self::North => "north",
        }
    }
}

/// Caller-owned policy for annex size, placement order, and blockage tolerance.
#[derive(Clone, Copy, Debug)]
pub struct Config<'a> {
    pub depth: i64,
    pub minimum_coordinate: i64,
    pub max_blocked_fraction: f32,
    pub side_order: &'a [Side],
}

/// Geometry the caller can turn into host-game construction work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub side: Side,
    pub fence_tiles: Vec<(i64, i64)>,
    pub gate_tile: (i64, i64),
    pub interior_tile: (i64, i64),
    pub new_rect: Rect,
}

/// Choose the first buildable annex in the caller's preferred side order.
pub fn plan(
    base: Rect,
    config: Config<'_>,
    mut is_blocked: impl FnMut(i64, i64) -> bool,
) -> Option<Plan> {
    for side in config.side_order {
        let (sx, sy) = match side {
            Side::East => (1, 0),
            Side::South => (0, 1),
            Side::West => (-1, 0),
            Side::North => (0, -1),
        };
        let (aminx, aminy, amaxx, amaxy) = match (sx, sy) {
            (1, 0) => (
                base.maxx + 1,
                base.miny,
                base.maxx + config.depth,
                base.maxy,
            ),
            (-1, 0) => (
                base.minx - config.depth,
                base.miny,
                base.minx - 1,
                base.maxy,
            ),
            (0, 1) => (
                base.minx,
                base.maxy + 1,
                base.maxx,
                base.maxy + config.depth,
            ),
            _ => (
                base.minx,
                base.miny - config.depth,
                base.maxx,
                base.miny - 1,
            ),
        };
        if aminx < config.minimum_coordinate || aminy < config.minimum_coordinate {
            continue;
        }

        let mut fence = Vec::new();
        for x in aminx..=amaxx {
            if sy != 1 {
                fence.push((x, aminy));
            }
            if sy != -1 {
                fence.push((x, amaxy));
            }
        }
        for y in (aminy + 1)..amaxy {
            if sx != 1 {
                fence.push((aminx, y));
            }
            if sx != -1 {
                fence.push((amaxx, y));
            }
        }

        let blocked = fence.iter().filter(|(x, y)| is_blocked(*x, *y)).count();
        if (blocked as f32) > (fence.len() as f32) * config.max_blocked_fraction {
            continue;
        }

        let gate_tile = match (sx, sy) {
            (1, 0) => (amaxx, (aminy + amaxy) / 2),
            (-1, 0) => (aminx, (aminy + amaxy) / 2),
            (0, 1) => ((aminx + amaxx) / 2, amaxy),
            _ => ((aminx + amaxx) / 2, aminy),
        };
        let interior_tile = ((aminx + amaxx) / 2, (aminy + amaxy) / 2);
        if is_blocked(interior_tile.0, interior_tile.1) {
            continue;
        }
        let fence_tiles = fence
            .into_iter()
            .filter(|tile| *tile != gate_tile && !is_blocked(tile.0, tile.1))
            .collect();

        return Some(Plan {
            side: *side,
            fence_tiles,
            gate_tile,
            interior_tile,
            new_rect: Rect {
                minx: base.minx.min(aminx),
                miny: base.miny.min(aminy),
                maxx: base.maxx.max(amaxx),
                maxy: base.maxy.max(amaxy),
            },
        });
    }
    None
}
