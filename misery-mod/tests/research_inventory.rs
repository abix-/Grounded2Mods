//! The complete inventory: every building part the game has
//! loaded, with its real measured size and where its position
//! marker sits.
//!
//! Sampling whatever square you are standing in answers the wrong
//! question. This walks every loaded static mesh so we know what
//! CAN be built with, not what happens to be nearby.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_inventory -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use serde_json::json;

/// A part, measured. Sizes are full extents in centimetres.
struct Part {
    name: String,
    w: f64,
    d: f64,
    h: f64,
    px: f64,
    py: f64,
    pz: f64,
}

/// Which part of a building this is, from its name. The game's
/// own naming is explicit enough to sort by.
fn category(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    if n.contains("walldoor") {
        "wall with a door"
    } else if n.contains("wallwindow") {
        "wall with a window"
    } else if n.contains("wallrounded") || n.contains("corner") {
        "corner"
    } else if n.starts_with("sm_wall_") && n.contains("broken") {
        "broken wall"
    } else if n.starts_with("sm_wall") {
        "wall"
    } else if n.starts_with("sm_floor") {
        "floor"
    } else if n.contains("ceiling") {
        "ceiling"
    } else if n.contains("stair") || n.contains("ladder") || n.contains("catwalk") {
        "stairs and walkways"
    } else if n.contains("pillar") || n.contains("post") || n.contains("beam") {
        "pillars and beams"
    } else if n.contains("doorframe") || n.contains("windowsframe") || n.contains("fakedoor") {
        "frames"
    } else if n.contains("fence") {
        "fences"
    } else if n.contains("road") {
        "road"
    } else {
        "other"
    }
}

/// Everything loaded, measured and sorted by what it is.
#[test]
fn full_inventory() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let r = api.op("mesh_info", json!({"prefix": ""}));
    assert!(r.ok, "mesh_info failed: {:?}", r.error);
    let meshes = r.result["meshes"].as_array().cloned().unwrap_or_default();
    println!("{} loaded mesh(es)\n", meshes.len());

    let g = |m: &serde_json::Value, k: &str, i: usize| {
        m[k].as_array()
            .and_then(|a| a.get(i).cloned())
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
    };
    let parts: Vec<Part> = meshes
        .iter()
        .map(|m| Part {
            name: m["name"].as_str().unwrap_or("?").to_string(),
            w: g(m, "size", 0),
            d: g(m, "size", 1),
            h: g(m, "size", 2),
            px: g(m, "pivot_offset", 0),
            py: g(m, "pivot_offset", 1),
            pz: g(m, "pivot_offset", 2),
        })
        .collect();

    // Building parts first, in the order you would use them.
    const ORDER: &[&str] = &[
        "wall",
        "wall with a door",
        "wall with a window",
        "broken wall",
        "corner",
        "floor",
        "ceiling",
        "pillars and beams",
        "stairs and walkways",
        "frames",
        "fences",
        "road",
    ];

    for cat in ORDER {
        let mut group: Vec<&Part> = parts.iter().filter(|p| category(&p.name) == *cat).collect();
        if group.is_empty() {
            continue;
        }
        group.sort_by(|a, b| a.name.cmp(&b.name));
        println!("## {cat} ({} part(s))", group.len());
        for p in group {
            println!(
                "  {:<40} {:>6.0} x {:>6.0} x {:>6.0} cm   marker {:>6.0} {:>6.0} {:>6.0}",
                p.name, p.w, p.d, p.h, p.px, p.py, p.pz
            );
        }
        println!();
    }

    let building: usize = ORDER
        .iter()
        .map(|c| parts.iter().filter(|p| category(&p.name) == *c).count())
        .sum();
    println!(
        "{building} building part(s) of {} loaded meshes ({} are props and scenery)",
        parts.len(),
        parts.len() - building
    );
}
