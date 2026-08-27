//! Discover MISERY route endpoints and Unreal's loaded navigation surface.
//!
//! This test does not move the player. It proves that the live player and
//! expedition door can supply route endpoints, then reports the engine
//! functions available for generating a navmesh path between them.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 k3sc cargo-lock test -p misery-mod \
//!   --test research_navigation -- --ignored --test-threads=1 --nocapture
//! ```

mod common;

use common::{api_or_skip, offsets_live, Api};
use modforge::client;
use serde_json::{json, Value};

const PLAYER_CLASS: &str = "BP_SGKMasterCharacter_C";
const EXPEDITION_DOOR_CLASS: &str = "BP_ExpeditionDoor_C";

fn actor_location(api: &Api, selector: &str) -> [f64; 3] {
    let (out, _) = api
        .call_ufunction("Actor", "K2_GetActorLocation", selector, &[0u8; 0x18])
        .expect("K2_GetActorLocation failed");
    assert_eq!(out.len(), 0x18, "K2_GetActorLocation parm size changed");
    [
        client::from_le_f64(&out, 0x00),
        client::from_le_f64(&out, 0x08),
        client::from_le_f64(&out, 0x10),
    ]
}

fn distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    left.into_iter()
        .zip(right)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt()
}

fn matching_functions(api: &Api, class: &str, needles: &[&str]) -> Value {
    let response = api.op("class_functions_by_name", json!({"class": class}));
    if !response.ok {
        return json!({"class": class, "error": response.error});
    }
    let functions = response.result["functions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|function| {
            let name = function["name"].as_str().unwrap_or_default();
            needles.iter().any(|needle| name.contains(needle))
        })
        .cloned()
        .collect::<Vec<_>>();
    json!({"class": class, "functions": functions})
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn put_f64(bytes: &mut [u8], offset: usize, value: f64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn find_navigation_path(api: &Api, world_context: u64, start: [f64; 3], goal: [f64; 3]) -> u64 {
    let cdo = client::find_class_cdo(api, "NavigationSystemV1")
        .expect("NavigationSystemV1 CDO is not loaded");
    let mut parms = [0u8; 80];
    put_u64(&mut parms, 0x00, world_context);
    for (index, value) in start.into_iter().enumerate() {
        put_f64(&mut parms, 0x08 + index * 8, value);
    }
    for (index, value) in goal.into_iter().enumerate() {
        put_f64(&mut parms, 0x20 + index * 8, value);
    }
    let (out, _) = api
        .call_ufunction(
            "NavigationSystemV1",
            "FindPathToLocationSynchronously",
            &cdo.addr_selector,
            &parms,
        )
        .expect("FindPathToLocationSynchronously failed");
    client::from_le_u64(&out, 0x48)
}

fn path_points(api: &Api, path: u64, start: [f64; 3], goal: [f64; 3]) -> Vec<[f64; 3]> {
    let object = client::read_bytes(api, path, 0, 0x100);
    for offset in (0x28..=0xf0).step_by(8) {
        let data = client::from_le_u64(&object, offset);
        let count = client::from_le_i32(&object, offset + 8);
        let capacity = client::from_le_i32(&object, offset + 12);
        if data == 0 || !(2..=256).contains(&count) || count > capacity || capacity > 4096 {
            continue;
        }
        let bytes = client::read_bytes(api, data, 0, count as u64 * 24);
        if bytes.len() != count as usize * 24 {
            continue;
        }
        let points = bytes
            .chunks_exact(24)
            .map(|point| {
                [
                    client::from_le_f64(point, 0),
                    client::from_le_f64(point, 8),
                    client::from_le_f64(point, 16),
                ]
            })
            .collect::<Vec<_>>();
        if points.iter().flatten().all(|value| value.is_finite())
            && distance(points[0], start) < 500.0
            && distance(points[points.len() - 1], goal) < 500.0
        {
            println!("navigation_path_points_offset=0x{offset:X}");
            return points;
        }
    }
    Vec::new()
}

#[test]
#[ignore = "reads route endpoints and loaded navigation functions from live MISERY"]
fn route_endpoints_and_navigation_surface_are_discoverable() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::find_live_instance(&api, PLAYER_CLASS)
        .expect("load a save so the live player exists");
    let start = actor_location(&api, &player.addr_selector);
    let door = client::walk_class_instances(&api, EXPEDITION_DOOR_CLASS, 32)
        .into_iter()
        .filter_map(|door| {
            let location = actor_location(&api, &door.addr_selector);
            let distance = distance(start, location);
            println!(
                "expedition_door_candidate={} location={location:?} distance={distance:.1}",
                door.full_name
            );
            (distance >= 100.0).then_some((distance, door, location))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .expect("the safe-area expedition door is not loaded");
    let goal = door.2;

    println!("player={} location={start:?}", player.full_name);
    println!("expedition_door={} location={goal:?}", door.1.full_name);
    assert!(
        start.into_iter().chain(goal).all(f64::is_finite),
        "route endpoint contains a non-finite coordinate"
    );

    for (class, needles) in [
        (
            "NavigationSystemV1",
            &["Path", "ProjectPoint", "NavigationSystem"][..],
        ),
        (
            "NavigationPath",
            &["Path", "Valid", "Partial", "Cost", "Length"][..],
        ),
    ] {
        println!(
            "navigation_surface={}",
            matching_functions(&api, class, needles)
        );
    }

    let path = find_navigation_path(&api, player.addr, start, goal);
    assert_ne!(path, 0, "Unreal did not create a navigation path");
    let (valid, _) = api
        .call_ufunction(
            "NavigationPath",
            "IsValid",
            &format!("addr:0x{path:X}"),
            &[0],
        )
        .expect("NavigationPath::IsValid failed");
    assert_eq!(valid, [1], "Unreal returned an invalid navigation path");

    let points = path_points(&api, path, start, goal);
    println!("navigation_path=0x{path:X} points={points:?}");
    assert!(
        points.len() >= 2,
        "could not identify the NavigationPath waypoint array"
    );
}
