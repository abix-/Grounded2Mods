//! What do the level designers' own rooms look like?
//!
//! Reads every modular kit piece placed in the live squares and
//! asks the questions a generator needs answered: which wall
//! heights and widths get used, do pieces land on a grid, how
//! many doors and windows per building, how big are the
//! enclosures, and do buildings share walls.
//!
//! Observation before generation: the rules should come from the
//! designers' work, not from assumptions about architecture.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_vanilla_rooms -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use modforge::client;
use serde_json::json;
use std::collections::BTreeMap;

type Api = common::Api;

struct KitPiece {
    mesh: String,
    x: f64,
    y: f64,
    z: f64,
    yaw: f64,
}

fn live_squares(api: &Api) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in client::walk_class_chain_instances(api, "BP_MasterAICharacter_C", 400) {
        let Some(path) = n.full_name.split(' ').nth(1) else { continue };
        let Some(sq) = path.split(".PersistentLevel").next() else { continue };
        let short = sq.rsplit('/').next().unwrap_or(sq).to_string();
        if short.contains('_') && !out.contains(&short) {
            out.push(short);
        }
    }
    out
}

fn kit_pieces(api: &Api, square: &str) -> Vec<KitPiece> {
    let r = api.op("kit_layout", json!({"level": square}));
    if !r.ok {
        return Vec::new();
    }
    r.result["pieces"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|p| KitPiece {
            mesh: p["mesh"].as_str().unwrap_or("?").to_string(),
            x: p["x"].as_f64().unwrap_or(0.0),
            y: p["y"].as_f64().unwrap_or(0.0),
            z: p["z"].as_f64().unwrap_or(0.0),
            yaw: p["yaw"].as_f64().unwrap_or(0.0),
        })
        .collect()
}

fn all_kit_pieces(api: &Api) -> Vec<KitPiece> {
    let mut all = Vec::new();
    for square in live_squares(api) {
        let pieces = kit_pieces(api, &square);
        if !pieces.is_empty() {
            println!("{square}: {} kit piece(s)", pieces.len());
        }
        all.extend(pieces);
    }
    all
}

/// Which parts of the kit the designers actually reach for.
#[test]
fn vanilla_part_usage() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let pieces = all_kit_pieces(&api);
    if pieces.is_empty() {
        println!("SKIP: no kit pieces in the live squares (try a town or factory)");
        return;
    }
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for p in &pieces {
        *counts.entry(p.mesh.clone()).or_default() += 1;
    }
    let mut rows: Vec<(&String, &usize)> = counts.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    println!("\n{} kit piece(s), {} distinct part(s):", pieces.len(), rows.len());
    for (mesh, n) in rows {
        println!("  {n:>4} x {mesh}");
    }

    // Which wall HEIGHT the designers build at, from the names.
    let mut heights: BTreeMap<i64, usize> = BTreeMap::new();
    for p in &pieces {
        if let Some(dims) = p.mesh.rsplit('_').next() {
            if let Some((_, h)) = dims.split_once('x') {
                let h: String = h.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(h) = h.parse::<i64>() {
                    *heights.entry(h).or_default() += 1;
                }
            }
        }
    }
    println!("\nwall/floor sizes in use (second dimension):");
    for (h, n) in &heights {
        println!("  {h:>5} cm: {n} piece(s)");
    }
}

/// Do kit pieces land on a grid, and which one?
#[test]
fn vanilla_grid_alignment() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let pieces = all_kit_pieces(&api);
    if pieces.is_empty() {
        println!("SKIP: no kit pieces live");
        return;
    }
    for grid in [400.0f64, 200.0, 100.0, 50.0] {
        let on = pieces
            .iter()
            .filter(|p| {
                let rx = (p.x.rem_euclid(grid)).min(grid - p.x.rem_euclid(grid));
                let ry = (p.y.rem_euclid(grid)).min(grid - p.y.rem_euclid(grid));
                rx < 1.0 && ry < 1.0
            })
            .count();
        println!(
            "{grid:>5.0} cm grid: {on}/{} pieces aligned ({:.0}%)",
            pieces.len(),
            100.0 * on as f64 / pieces.len() as f64
        );
    }

    // Which yaws appear: are walls axis-aligned or free-angled?
    let mut yaws: BTreeMap<i64, usize> = BTreeMap::new();
    for p in &pieces {
        *yaws.entry(p.yaw.rem_euclid(360.0).round() as i64).or_default() += 1;
    }
    println!("\nyaw values in use:");
    for (y, n) in yaws.iter().take(20) {
        println!("  {y:>4} deg: {n} piece(s)");
    }
}

/// Group kit pieces into buildings and describe each: footprint,
/// wall count, openings, storeys.
#[test]
fn vanilla_room_shapes() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let pieces = all_kit_pieces(&api);
    if pieces.is_empty() {
        println!("SKIP: no kit pieces live");
        return;
    }

    // Greedy spatial grouping: pieces within 12 m of a group's
    // seed belong to the same building.
    const RADIUS: f64 = 1200.0;
    let mut taken = vec![false; pieces.len()];
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for i in 0..pieces.len() {
        if taken[i] {
            continue;
        }
        let mut members = vec![i];
        taken[i] = true;
        // Grow the group: anything near any member joins.
        let mut cursor = 0;
        while cursor < members.len() {
            let seed = &pieces[members[cursor]];
            for j in 0..pieces.len() {
                if taken[j] {
                    continue;
                }
                let dx = pieces[j].x - seed.x;
                let dy = pieces[j].y - seed.y;
                if dx * dx + dy * dy <= RADIUS * RADIUS {
                    taken[j] = true;
                    members.push(j);
                }
            }
            cursor += 1;
        }
        if members.len() >= 4 {
            groups.push(members);
        }
    }

    println!("\n{} building(s) of 4+ kit pieces:", groups.len());
    let mut footprints: Vec<(f64, f64)> = Vec::new();
    for (n, members) in groups.iter().enumerate().take(12) {
        let xs: Vec<f64> = members.iter().map(|&i| pieces[i].x).collect();
        let ys: Vec<f64> = members.iter().map(|&i| pieces[i].y).collect();
        let zs: Vec<f64> = members.iter().map(|&i| pieces[i].z).collect();
        let w = xs.iter().cloned().fold(f64::MIN, f64::max)
            - xs.iter().cloned().fold(f64::MAX, f64::min);
        let l = ys.iter().cloned().fold(f64::MIN, f64::max)
            - ys.iter().cloned().fold(f64::MAX, f64::min);
        let z_lo = zs.iter().cloned().fold(f64::MAX, f64::min);
        let z_hi = zs.iter().cloned().fold(f64::MIN, f64::max);
        footprints.push((w, l));

        let doors = members
            .iter()
            .filter(|&&i| pieces[i].mesh.contains("Door"))
            .count();
        let windows = members
            .iter()
            .filter(|&&i| pieces[i].mesh.contains("Window"))
            .count();
        let floors = members
            .iter()
            .filter(|&&i| pieces[i].mesh.starts_with("SM_Floor"))
            .count();
        let walls = members.len() - floors;
        // Distinct floor heights: storeys.
        let mut levels: Vec<i64> = members
            .iter()
            .map(|&i| (pieces[i].z / 100.0).round() as i64)
            .collect();
        levels.sort_unstable();
        levels.dedup();

        println!(
            "  building {n}: {} pieces, {w:.0} x {l:.0} cm footprint, z {z_lo:.0}..{z_hi:.0}",
            members.len()
        );
        println!(
            "    {walls} wall(s), {floors} floor(s), {doors} door(s), {windows} window(s), {} distinct height(s)",
            levels.len()
        );
    }

    if !footprints.is_empty() {
        let avg_w: f64 = footprints.iter().map(|f| f.0).sum::<f64>() / footprints.len() as f64;
        let avg_l: f64 = footprints.iter().map(|f| f.1).sum::<f64>() / footprints.len() as f64;
        println!("\naverage footprint: {avg_w:.0} x {avg_l:.0} cm");
    }
}
