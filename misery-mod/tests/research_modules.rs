//! What a loaded square HOLDS: the placed parts, visible before
//! any conclusion is drawn from them.
//!
//! The stud catalog itself is read from asset-loaded levels, not
//! from the squares around the player; that lives in
//! `research_assets.rs`. This file is the eyes-on check of what
//! a live square's level read returns.
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
