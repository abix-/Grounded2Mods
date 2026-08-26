//! Harvest and compose: reading a square's pieces and building
//! new places out of them (src/harvest.rs).
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_harvest -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use modforge::client;
use serde_json::json;

type Api = common::Api;

/// Live square names (from the NPC census), newest world first.
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

/// What is a square actually made of? Class histogram per live
/// square: the parts inventory for composing new places.
#[test]
fn square_parts_inventory() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let squares = live_squares(&api);
    if squares.is_empty() {
        println!("SKIP: no live squares (enter an expedition)");
        return;
    }
    for square in squares.iter().take(3) {
        let r = api.op("harvest_classes", json!({"level": square}));
        if !r.ok {
            println!("{square}: FAILED {:?}", r.error);
            continue;
        }
        println!(
            "=== {square}: {} actors ===",
            r.result["actors"].as_u64().unwrap_or(0)
        );
        if let Some(classes) = r.result["classes"].as_array() {
            for c in classes.iter().take(25) {
                println!(
                    "  {:>4} x {}",
                    c["count"].as_u64().unwrap_or(0),
                    c["class"].as_str().unwrap_or("?")
                );
            }
            if classes.len() > 25 {
                println!("  ... {} more class(es)", classes.len() - 25);
            }
        }
    }
}

/// Round trip: harvest a square's pieces and rebuild a slice of
/// them next to the player. Proves pieces can be read AND put
/// back, which is the foundation for generating new places.
#[test]
#[ignore = "spawns actors in the live game"]
fn harvest_and_compose() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let squares = live_squares(&api);
    let Some(square) = squares.first().cloned() else {
        println!("SKIP: no live squares");
        return;
    };

    let h = api.op("harvest_square", json!({"level": square}));
    assert!(h.ok, "harvest failed: {:?}", h.error);
    let all = h.result["pieces"].as_array().cloned().unwrap_or_default();
    println!("harvested {} piece(s) from {square}", all.len());

    // Take the pieces nearest the square centre so the rebuild is
    // a dense cluster rather than a scattering of far-flung props.
    let mut near: Vec<serde_json::Value> = all.clone();
    near.sort_by(|a, b| {
        let d = |v: &serde_json::Value| {
            let x = v["dx"].as_f64().unwrap_or(0.0);
            let y = v["dy"].as_f64().unwrap_or(0.0);
            x * x + y * y
        };
        d(a).partial_cmp(&d(b)).unwrap_or(std::cmp::Ordering::Equal)
    });
    near.truncate(60);
    let with_mesh = near.iter().filter(|p| p["mesh"].is_string()).count();
    println!("composing {} piece(s), {with_mesh} with meshes", near.len());

    let comp = json!({"source": square, "pieces": near});
    // Offset so the rebuild lands beside the player, not inside.
    let r = api.op("compose", json!({"composition": comp, "max": 60}));
    assert!(r.ok, "compose failed: {:?}", r.error);
    println!("compose result: {}", r.result);
    let placed = r.result["placed"].as_u64().unwrap_or(0);
    assert!(placed > 0, "nothing was placed: {}", r.result);
}

/// The piece vocabulary: which MESHES a square is built from,
/// most used first. Decides whether new structures can be
/// assembled from parts (walls, roofs, floors) or only from
/// whole captured buildings.
#[test]
fn piece_vocabulary() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let squares = live_squares(&api);
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut squares_read = 0;
    for square in squares.iter().take(4) {
        let r = api.op("harvest_square", json!({"level": square}));
        if !r.ok {
            continue;
        }
        squares_read += 1;
        for p in r.result["pieces"].as_array().cloned().unwrap_or_default() {
            let name = match p["mesh"].as_str() {
                Some(m) => m.to_string(),
                None => format!("(bp) {}", p["class"].as_str().unwrap_or("?")),
            };
            *counts.entry(name).or_default() += 1;
        }
    }
    let mut rows: Vec<(String, usize)> = counts.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    println!(
        "{} distinct piece(s) across {squares_read} square(s)",
        rows.len()
    );
    for (name, n) in rows.iter().take(45) {
        println!("  {n:>4} x {name}");
    }
}

/// The same squares read by SHAPE rather than name: how many
/// walls, floors, posts a place is built from, and their real
/// dimensions. This is the data that makes new structures
/// assemblable rather than only copyable.
#[test]
fn piece_shapes() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let squares = live_squares(&api);
    let mut by_shape: std::collections::HashMap<String, Vec<(String, f64, f64, f64)>> =
        std::collections::HashMap::new();
    let mut unmeasured = 0usize;

    for square in squares.iter().take(3) {
        let r = api.op("harvest_square", json!({"level": square}));
        if !r.ok {
            continue;
        }
        for p in r.result["pieces"].as_array().cloned().unwrap_or_default() {
            // Half-extents, UE cm, converted to metre full sizes.
            let ex = p["ex"].as_f64().unwrap_or(0.0) / 50.0;
            let ey = p["ey"].as_f64().unwrap_or(0.0) / 50.0;
            let ez = p["ez"].as_f64().unwrap_or(0.0) / 50.0;
            if ex <= 0.0 && ey <= 0.0 && ez <= 0.0 {
                unmeasured += 1;
                continue;
            }
            // Classify with the same rules as modforge::structure,
            // reading UE axes (z is up here).
            let hmax = ex.max(ey);
            let hmin = ex.min(ey);
            let shape = if ex < 0.7 && ey < 0.7 && ez < 0.7 {
                "clutter"
            } else if ez * 4.0 < hmax && hmin * 3.0 > hmax {
                "slab"
            } else if hmin * 4.0 < hmax && ez * 2.0 > hmax {
                "panel"
            } else if ez > hmax * 2.0 {
                "post"
            } else if hmax > hmin * 4.0 && hmax > ez * 2.0 {
                "beam"
            } else {
                "block"
            };
            let name = p["mesh"].as_str().unwrap_or("(bp)").to_string();
            by_shape
                .entry(shape.to_string())
                .or_default()
                .push((name, ex, ey, ez));
        }
    }

    let mut shapes: Vec<(&String, &Vec<(String, f64, f64, f64)>)> = by_shape.iter().collect();
    shapes.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    println!("{unmeasured} piece(s) had no measurable mesh");
    for (shape, items) in shapes {
        println!("=== {shape}: {} piece(s) ===", items.len());
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (name, x, y, z) in items.iter() {
            if seen.insert(name.as_str()) {
                println!("  {name:<44} {x:.1} x {y:.1} x {z:.1} m");
            }
            if seen.len() >= 12 {
                break;
            }
        }
    }
}

/// Harvest one square into a composition and report its shape.
#[test]
fn harvest_one_square() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let squares = live_squares(&api);
    let Some(square) = squares.first() else {
        println!("SKIP: no live squares");
        return;
    };
    let r = api.op("harvest_square", json!({"level": square}));
    assert!(r.ok, "harvest failed: {:?}", r.error);
    let pieces = r.result["pieces"].as_array().cloned().unwrap_or_default();
    println!("harvested {} piece(s) from {square}", pieces.len());
    for p in pieces.iter().take(15) {
        println!(
            "  {:<40} d=({:.0}, {:.0}, {:.0}) yaw={:.0} scale={:.2}",
            p["class"].as_str().unwrap_or("?"),
            p["dx"].as_f64().unwrap_or(0.0),
            p["dy"].as_f64().unwrap_or(0.0),
            p["dz"].as_f64().unwrap_or(0.0),
            p["yaw"].as_f64().unwrap_or(0.0),
            p["scale"].as_f64().unwrap_or(0.0),
        );
    }
    let xs: Vec<f64> = pieces.iter().filter_map(|p| p["dx"].as_f64()).collect();
    let ys: Vec<f64> = pieces.iter().filter_map(|p| p["dy"].as_f64()).collect();
    if !xs.is_empty() {
        println!(
            "extent: x {:.0}..{:.0}  y {:.0}..{:.0}",
            xs.iter().cloned().fold(f64::MAX, f64::min),
            xs.iter().cloned().fold(f64::MIN, f64::max),
            ys.iter().cloned().fold(f64::MAX, f64::min),
            ys.iter().cloned().fold(f64::MIN, f64::max),
        );
    }
}
