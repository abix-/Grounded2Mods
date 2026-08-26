//! The modular building kit: which parts are loaded, their real
//! sizes, and where each mesh's pivot sits (worldgen.md 9.5).
//!
//! Pivot placement decides every coordinate in a room layout, so
//! it is measured, not assumed.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_kit -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use serde_json::json;

type Api = common::Api;

/// Kit families worth knowing about, by mesh name prefix.
const FAMILIES: &[&str] = &[
    "SM_Wall_",
    "SM_WallDoor",
    "SM_WallWindow",
    "SM_WallRounded",
    "SM_Floor_",
    "SM_Stair",
    "SM_Pillar",
    "SM_Concrete_",
];

fn dump(api: &Api, prefix: &str) -> usize {
    let r = api.op("mesh_info", json!({"prefix": prefix}));
    if !r.ok {
        println!("=== {prefix}: FAILED {:?} ===", r.error);
        return 0;
    }
    let meshes = r.result["meshes"].as_array().cloned().unwrap_or_default();
    println!("=== {prefix}: {} loaded ===", meshes.len());
    for m in &meshes {
        let name = m["name"].as_str().unwrap_or("?");
        let s = m["size"].as_array().cloned().unwrap_or_default();
        let p = m["pivot_offset"].as_array().cloned().unwrap_or_default();
        let f = |v: &Vec<serde_json::Value>, i: usize| {
            v.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0)
        };
        println!(
            "  {name:<34} size {:>6.0} x {:>5.0} x {:>6.0}   pivot {:>6.0} {:>5.0} {:>6.0}",
            f(&s, 0), f(&s, 1), f(&s, 2),
            f(&p, 0), f(&p, 1), f(&p, 2),
        );
    }
    meshes.len()
}

/// What the kit offers right now, with sizes and pivots.
#[test]
fn kit_inventory() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let mut total = 0;
    for prefix in FAMILIES {
        total += dump(&api, prefix);
    }
    println!("{total} kit mesh(es) loaded in total");
}

/// The pivot rule the room builder depends on: for a
/// `SM_Wall_<width>x<height>` mesh, the pivot sits at the bottom
/// start corner, so the box centre is at (+width/2, 0, +height/2)
/// and the name's numbers ARE the real dimensions.
#[test]
fn wall_pivot_rule_holds() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let r = api.op("mesh_info", json!({"prefix": "SM_Wall_"}));
    assert!(r.ok, "mesh_info failed: {:?}", r.error);
    let meshes = r.result["meshes"].as_array().cloned().unwrap_or_default();
    assert!(!meshes.is_empty(), "no SM_Wall_ meshes loaded");

    let mut checked = 0;
    for m in &meshes {
        let name = m["name"].as_str().unwrap_or("");
        // Only the dimension-named ones make the promise.
        let Some(dims) = name.strip_prefix("SM_Wall_") else { continue };
        let Some((w, h)) = dims.split_once('x') else { continue };
        let (Ok(w), Ok(h)) = (
            w.parse::<f64>(),
            h.trim_end_matches(|c: char| !c.is_ascii_digit()).parse::<f64>(),
        ) else {
            continue;
        };
        let g = |k: &str, i: usize| {
            m[k].as_array()
                .and_then(|a| a.get(i).cloned())
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
        };
        let (sx, sz) = (g("size", 0), g("size", 2));
        let (px, pz) = (g("pivot_offset", 0), g("pivot_offset", 2));
        println!("{name}: name says {w}x{h}, mesh is {sx:.0}x{sz:.0}, pivot {px:.0},{pz:.0}");
        assert!((sx - w).abs() < 2.0, "{name}: width {sx} != named {w}");
        assert!((sz - h).abs() < 2.0, "{name}: height {sz} != named {h}");
        assert!(
            (px - w / 2.0).abs() < 2.0,
            "{name}: pivot x {px} is not half the width {w}"
        );
        assert!(
            (pz - h / 2.0).abs() < 2.0,
            "{name}: pivot z {pz} is not half the height {h}"
        );
        checked += 1;
    }
    assert!(checked >= 6, "only {checked} dimension-named walls checked");
    println!("{checked} wall(s) obey the pivot rule");
}
