//! Where do two pieces MEET?
//!
//! Those contact points are the studs. Not the mesh's bounding
//! box, which lies: `SM_Floor_400x400` measures 5.18 by 5.73 m
//! because it has a lip, and measuring the mesh more precisely
//! does not help, because the lip is really there (pieces.md).
//! Not the mesh's name either, which we do not trust.
//!
//! What we trust is where the designers put things. If a wall
//! always sits 400 cm along from the next wall, that offset is a
//! stud. If a wall always sits 200 cm out and 150 cm up from a
//! floor tile, that is another one. The offsets that RECUR
//! between a pair of meshes are the ways those two pieces are
//! allowed to join.
//!
//! So this measures, for every pair of placed pieces near enough
//! to touch, the offset from one to the other and their relative
//! facing. Then it counts. The common ones are the instruction
//! booklet.
//!
//! Read-only, and it needs a world loaded.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_modules -- --test-threads=1 --nocapture
//! ```

mod common;
use common::api_or_skip;
use serde_json::json;
use std::collections::HashMap;

/// Two pieces further apart than this are not touching, and the
/// offset between them says nothing about how they join. Metres,
/// and generous: the biggest kit part is 4 m, so 9 m still
/// catches a large piece meeting a large piece corner to corner.
const TOUCHING: f64 = 9.0;

/// Offsets are rounded to this before counting, so a hand-nudged
/// placement does not invent a new stud. Centimetres. One
/// centimetre is far finer than any module, and coarse enough
/// that float noise collapses.
const ROUND_CM: f64 = 1.0;

/// A recurring offset needs at least this many sightings to be
/// called a stud rather than a coincidence.
const MIN_SIGHTINGS: usize = 4;

/// One way two meshes are seen to join.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Join {
    /// The piece being joined FROM.
    from: String,
    /// The piece being joined TO.
    to: String,
    /// Offset from one origin to the other, centimetres, rounded.
    /// In this crate's convention: metres, y up, so y is height.
    dx: i64,
    dy: i64,
    dz: i64,
    /// How much the second piece is turned relative to the first,
    /// degrees, rounded to the nearest 15. A stud is as much about
    /// facing as position: a wall meeting a wall end-on is a
    /// different join from the same wall turned across it.
    turn: i64,
}

/// Every way the designers join two pieces, counted.
#[test]
fn where_two_pieces_meet() {
    let Some(api) = api_or_skip() else { return };
    let squares = loaded_squares(&api);
    if squares.is_empty() {
        println!("no map squares loaded; load a save");
        return;
    }
    println!("{} square(s) loaded", squares.len());

    let mut joins: HashMap<Join, usize> = HashMap::new();
    let mut read = 0usize;

    for square in squares.iter().take(4) {
        let r = api.op("level_pieces", json!({ "level": square }));
        if !r.ok {
            println!("  {}: {:?}", short(square), r.error);
            continue;
        }
        let pieces: Vec<Placed> = r.result["pieces"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(placed)
            .collect();
        println!("  {} placed pieces in {}", pieces.len(), short(square));
        read += pieces.len();

        // Every ordered pair close enough to be touching. Ordered,
        // because "a wall 2 m from a floor" and "a floor 2 m from
        // a wall" are the same join seen from each end, and we
        // want both so either piece can be the one already placed.
        for a in &pieces {
            for b in &pieces {
                if std::ptr::eq(a, b) {
                    continue;
                }
                let (dx, dy, dz) = (b.x - a.x, b.y - a.y, b.z - a.z);
                let far = (dx * dx + dy * dy + dz * dz).sqrt();
                if far > TOUCHING || far <= 0.0 {
                    continue;
                }
                joins
                    .entry(Join {
                        from: a.mesh.clone(),
                        to: b.mesh.clone(),
                        dx: cm(dx),
                        dy: cm(dy),
                        dz: cm(dz),
                        turn: turn_of(b.yaw - a.yaw),
                    })
                    .and_modify(|n| *n += 1)
                    .or_insert(1);
            }
        }
    }

    let mut rows: Vec<(&Join, &usize)> = joins
        .iter()
        .filter(|(_, n)| **n >= MIN_SIGHTINGS)
        .collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));

    println!(
        "\n{read} placed pieces, {} distinct joins, {} seen {MIN_SIGHTINGS}+ times\n",
        joins.len(),
        rows.len()
    );
    println!(
        "{:<30} {:<30} {:>7} {:>7} {:>7} {:>6} {:>6}",
        "from", "to", "dx cm", "dy cm", "dz cm", "turn", "seen"
    );
    for (j, n) in rows.iter().take(40) {
        println!(
            "{:<30} {:<30} {:>7} {:>7} {:>7} {:>6} {:>6}",
            trim(&j.from),
            trim(&j.to),
            j.dx,
            j.dy,
            j.dz,
            j.turn,
            n
        );
    }

    assert!(
        !rows.is_empty(),
        "no offset recurred, so either nothing is modular or the read is wrong"
    );
}

/// The same joins, but only between two DIFFERENT meshes.
///
/// A piece next to another of its own kind gives the grid pitch.
/// A piece next to a different kind is the more interesting half:
/// it says a wall may sit on a floor, and exactly where.
#[test]
fn where_two_different_pieces_meet() {
    let Some(api) = api_or_skip() else { return };
    let squares = loaded_squares(&api);
    if squares.is_empty() {
        println!("no map squares loaded; load a save");
        return;
    }

    let mut joins: HashMap<Join, usize> = HashMap::new();
    for square in squares.iter().take(4) {
        let r = api.op("level_pieces", json!({ "level": square }));
        if !r.ok {
            continue;
        }
        let pieces: Vec<Placed> = r.result["pieces"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(placed)
            .collect();
        for a in &pieces {
            for b in &pieces {
                if a.mesh == b.mesh {
                    continue;
                }
                let (dx, dy, dz) = (b.x - a.x, b.y - a.y, b.z - a.z);
                let far = (dx * dx + dy * dy + dz * dz).sqrt();
                if far > TOUCHING || far <= 0.0 {
                    continue;
                }
                joins
                    .entry(Join {
                        from: a.mesh.clone(),
                        to: b.mesh.clone(),
                        dx: cm(dx),
                        dy: cm(dy),
                        dz: cm(dz),
                        turn: turn_of(b.yaw - a.yaw),
                    })
                    .and_modify(|n| *n += 1)
                    .or_insert(1);
            }
        }
    }

    let mut rows: Vec<(&Join, &usize)> = joins
        .iter()
        .filter(|(_, n)| **n >= MIN_SIGHTINGS)
        .collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));

    println!("\n{} joins between DIFFERENT meshes, seen {MIN_SIGHTINGS}+ times\n", rows.len());
    println!(
        "{:<30} {:<30} {:>7} {:>7} {:>7} {:>6} {:>6}",
        "from", "to", "dx cm", "dy cm", "dz cm", "turn", "seen"
    );
    for (j, n) in rows.iter().take(40) {
        println!(
            "{:<30} {:<30} {:>7} {:>7} {:>7} {:>6} {:>6}",
            trim(&j.from),
            trim(&j.to),
            j.dx,
            j.dy,
            j.dz,
            j.turn,
            n
        );
    }
}

/// What a square holds, so the data is visible before any
/// conclusion is drawn from it.
#[test]
fn what_one_square_holds() {
    let Some(api) = api_or_skip() else { return };
    let Some(square) = loaded_squares(&api).into_iter().next() else {
        println!("no map squares loaded; load a save");
        return;
    };
    let r = api.op("level_pieces", json!({ "level": square }));
    assert!(r.ok, "level_pieces failed: {:?}", r.error);
    let pieces = r.result["pieces"].as_array().cloned().unwrap_or_default();
    println!("{} pieces in {}\n", pieces.len(), short(&square));

    let mut by_mesh: HashMap<String, usize> = HashMap::new();
    for p in &pieces {
        *by_mesh
            .entry(p["asset"].as_str().unwrap_or("<none>").to_string())
            .or_default() += 1;
    }
    let mut rows: Vec<_> = by_mesh.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    for (mesh, n) in rows.iter().take(20) {
        println!("{n:>5}  {mesh}");
    }

    println!("\nfirst few, with position and facing:");
    for p in pieces.iter().take(6) {
        println!(
            "  {:<34} at {:?} yaw {}",
            p["asset"].as_str().unwrap_or("<none>"),
            p["offset"],
            p["yaw"]
        );
    }
}

/// One placed piece: what it is, where it is, which way it faces.
struct Placed {
    mesh: String,
    x: f64,
    y: f64,
    z: f64,
    yaw: f64,
}

fn placed(v: &serde_json::Value) -> Option<Placed> {
    let mesh = v["asset"].as_str()?.to_string();
    let o = v["offset"].as_array()?;
    if o.len() < 3 {
        return None;
    }
    Some(Placed {
        mesh,
        x: o[0].as_f64()?,
        y: o[1].as_f64()?,
        z: o[2].as_f64()?,
        yaw: v["yaw"].as_f64().unwrap_or(0.0),
    })
}

/// Metres to rounded centimetres.
fn cm(m: f64) -> i64 {
    ((m * 100.0) / ROUND_CM).round() as i64
}

/// Radians of relative turn to degrees, rounded to 15, folded
/// into 0..360. Kit parts turn in right angles; 15 is fine enough
/// to notice if something does not.
fn turn_of(radians: f64) -> i64 {
    let deg = radians.to_degrees();
    let snapped = (deg / 15.0).round() * 15.0;
    ((snapped as i64 % 360) + 360) % 360
}

/// The squares currently loaded, as level paths.
fn loaded_squares(api: &common::Api) -> Vec<String> {
    let r = api.op("walk_class_chain", json!({ "needle": "Level", "max": 512 }));
    if !r.ok {
        return Vec::new();
    }
    r.result["instances"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|i| i["full_name"].as_str())
                .filter(|n| n.starts_with("Level ") && n.contains("WorldPresets"))
                .filter_map(|n| n.split(' ').nth(1))
                .filter_map(|p| p.split(".PersistentLevel").next())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn short(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Mesh names are long and the interesting part is the end.
fn trim(mesh: &str) -> String {
    if mesh.len() <= 30 {
        mesh.to_string()
    } else {
        format!("...{}", &mesh[mesh.len() - 27..])
    }
}
