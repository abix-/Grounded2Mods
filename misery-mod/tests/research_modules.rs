//! Where do two parts MEET?
//!
//! Those contact points are the studs. Not the mesh's bounding
//! box, which lies: `SM_Floor_400x400` measures 5.18 by 5.73 m
//! because it has a lip, and measuring the mesh more precisely
//! does not help, because the lip is really there (parts.md).
//! Not the mesh's name either, which we do not trust.
//!
//! What we trust is where the designers put things. If a wall
//! always sits 400 cm along from the next wall, that offset is a
//! stud. If a wall always sits 200 cm out and 150 cm up from a
//! floor tile, that is another one. The offsets that RECUR
//! between a pair of meshes are the ways those two parts are
//! allowed to join.
//!
//! So this measures, for every pair of placed parts near enough
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

/// Two parts further apart than this are not touching, and the
/// offset between them says nothing about how they join. Metres,
/// and generous: the biggest kit part is 4 m, so 9 m still
/// catches a large part meeting a large part corner to corner.
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
    /// The part being joined FROM.
    from: String,
    /// The part being joined TO.
    to: String,
    /// Offset from one origin to the other, centimetres, rounded.
    /// In this crate's convention: metres, y up, so y is height.
    dx: i64,
    dy: i64,
    dz: i64,
    /// How much the second part is turned relative to the first,
    /// degrees, rounded to the nearest 15. A stud is as much about
    /// facing as position: a wall meeting a wall end-on is a
    /// different join from the same wall turned across it.
    turn: i64,
}

/// Every way the designers join two parts, counted.
#[test]
fn where_two_parts_meet() {
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
        let r = api.op("level_parts", json!({ "level": square }));
        if !r.ok {
            println!("  {}: {:?}", short(square), r.error);
            continue;
        }
        let parts: Vec<Placed> = r.result["parts"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(placed)
            .collect();
        println!("  {} placed parts in {}", parts.len(), short(square));
        read += parts.len();

        // Every ordered pair close enough to be touching. Ordered,
        // because "a wall 2 m from a floor" and "a floor 2 m from
        // a wall" are the same join seen from each end, and we
        // want both so either part can be the one already placed.
        for a in &parts {
            for b in &parts {
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
        "\n{read} placed parts, {} distinct joins, {} seen {MIN_SIGHTINGS}+ times\n",
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
/// A part next to another of its own kind gives the grid pitch.
/// A part next to a different kind is the more interesting half:
/// it says a wall may sit on a floor, and exactly where.
#[test]
fn where_two_different_parts_meet() {
    let Some(api) = api_or_skip() else { return };
    let squares = loaded_squares(&api);
    if squares.is_empty() {
        println!("no map squares loaded; load a save");
        return;
    }

    let mut joins: HashMap<Join, usize> = HashMap::new();
    for square in squares.iter().take(4) {
        let r = api.op("level_parts", json!({ "level": square }));
        if !r.ok {
            continue;
        }
        let parts: Vec<Placed> = r.result["parts"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(placed)
            .collect();
        for a in &parts {
            for b in &parts {
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
    let r = api.op("level_parts", json!({ "level": square }));
    assert!(r.ok, "level_parts failed: {:?}", r.error);
    let parts = r.result["parts"].as_array().cloned().unwrap_or_default();
    println!("{} parts in {}\n", parts.len(), short(&square));

    let mut by_mesh: HashMap<String, usize> = HashMap::new();
    for p in &parts {
        *by_mesh
            .entry(p["asset"].as_str().unwrap_or("<none>").to_string())
            .or_default() += 1;
    }
    let mut rows: Vec<_> = by_mesh.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    for (mesh, n) in rows.iter().take(20) {
        println!("{n:>5}  {mesh}");
    }

    println!("\nfirst few, with position, facing, size and pivot:");
    for p in parts.iter().take(6) {
        println!(
            "  {:<34} at {:?} yaw {} extent {:?} pivot {:?}",
            p["asset"].as_str().unwrap_or("<none>"),
            p["offset"],
            p["yaw"],
            p["extent"],
            p["pivot"]
        );
    }

    // A part read without its pivot is a part whose faces cannot
    // be placed, so this is the thing to fail on. A pivot may
    // legitimately be zero, but not every one of them: the floor
    // tiles alone are placed at a corner.
    let with_pivot = parts
        .iter()
        .filter(|p| {
            p["pivot"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v.as_f64().unwrap_or(0.0) != 0.0))
        })
        .count();
    println!("\n{with_pivot} of {} parts carry a pivot", parts.len());
    assert!(with_pivot > 0, "no placed part came back with a pivot");

    // A broken actor must be SKIPPED, never measured: reading
    // through a mesh pointer that does not resolve produced an
    // extent of 6.8e36 and NaN before the class check. Largest
    // real part is the 2 km mountain backdrop, so 10 km bounds
    // every honest measurement.
    for p in &parts {
        assert_ne!(
            p["asset"].as_str(),
            Some("<bogus-fname>"),
            "an unresolvable mesh was measured instead of skipped"
        );
        for key in ["extent", "pivot"] {
            for v in p[key].as_array().into_iter().flatten() {
                let n = v.as_f64();
                assert!(
                    n.is_some_and(|n| n.abs() < 10_000.0),
                    "{} carries a broken {key} {:?}",
                    p["asset"].as_str().unwrap_or("<none>"),
                    p[key]
                );
            }
        }
    }
}

/// One placed part: what it is, where it is, which way it faces.
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

/// THE STUDS, in two halves.
///
/// The game writes SIGHTINGS to a file, because a single pass
/// over eleven squares is 50,000 of them and 12 MB of JSON, and
/// because evidence accumulates across sessions.
///
/// Deriving studs from that file needs no game at all, which is
/// the point of keeping the two apart: a threshold changed later
/// re-derives from what is already on disk.
#[test]
fn derive_the_studs() {
    let Some(api) = api_or_skip() else { return };
    let squares = loaded_squares(&api);
    if squares.is_empty() {
        println!("no map squares loaded; load a save");
        return;
    }

    // Reading eleven levels and comparing every pair takes longer
    // than the client's usual few seconds. This is a research
    // sweep, not a control the game waits on.
    let api = api.with_timeout(std::time::Duration::from_secs(120));

    let dir = "C:/Games/Steam/steamapps/common/MISERY/MISERY/Binaries/Win64/ue4ss/Mods/MiseryMod/dlls";
    let sightings_path = format!("{dir}/sightings.json");
    let r = api.op(
        "joins",
        json!({ "levels": squares, "touching": TOUCHING, "path": sightings_path }),
    );
    assert!(r.ok, "joins failed: {:?}", r.error);
    println!(
        "{} sightings from {} squares -> {}",
        r.result["sightings"], r.result["levels"], sightings_path
    );

    // From here on the game is not involved.
    let text = std::fs::read_to_string(&sightings_path).expect("sightings file");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("sightings json");
    let joins: Vec<modforge::studs::Join> = doc["joins"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|j| {
            let o = j["offset"].as_array()?;
            Some(modforge::studs::Join {
                from: j["from"].as_str()?.to_string(),
                to: j["to"].as_str()?.to_string(),
                offset: (o[0].as_f64()?, o[1].as_f64()?, o[2].as_f64()?),
                from_yaw: j["from_yaw"].as_f64()?,
                to_yaw: j["to_yaw"].as_f64()?,
            })
        })
        .collect();

    let studs = modforge::studs::derive(&joins, modforge::studs::Derive::default());
    println!("{} parts have studs
", studs.len());

    let mut names: Vec<&String> = studs.keys().collect();
    names.sort_by_key(|n| std::cmp::Reverse(studs[*n].iter().map(|s| s.seen).sum::<usize>()));
    for name in names.iter().take(5) {
        let list = &studs[*name];
        println!("{name}  ({} studs)", list.len());
        for st in list.iter().take(5) {
            let mut w: Vec<(&String, &usize)> = st.with.iter().collect();
            w.sort_by(|a, b| b.1.cmp(a.1));
            let who: Vec<String> = w.iter().take(2).map(|(m, n)| format!("{m} x{n}")).collect();
            println!(
                "   at ({:>6.2},{:>6.2},{:>6.2})  turn {:>3}  seen {:>4}  with {}",
                st.at.0, st.at.1, st.at.2, st.turn, st.seen, who.join(", ")
            );
        }
        println!();
    }
    assert!(!studs.is_empty(), "no part got a stud");
}
