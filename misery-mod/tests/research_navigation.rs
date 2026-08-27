//! Walk from the live MISERY player into an expedition through Unreal navigation.
//!
//! Unreal owns the navigation mesh and drives its player-controller path
//! follower between three meaningful stops. The bot presses the same interaction
//! key as the player at both doors and retains dense positions only as diagnostics.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 k3sc cargo-lock test -p misery-mod \
//!   --test research_navigation -- --ignored --test-threads=1 --nocapture
//! ```

mod common;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use common::{Api, api_or_skip, offsets_live};
use modforge::client;
use modforge::route::{Position, RouteEdge, RouteGraph, StuckDetector, Waypoint};
use serde_json::{Value, json};

const PLAYER_CLASS: &str = "BP_SGKMasterCharacter_C";
const METAL_DOOR_CLASS: &str = "BP_MetalDoor_C";
const EXPEDITION_DOOR_CLASS: &str = "BP_ExpeditionDoor_C";
const LOOT_BOX_CLASSES: [&str; 4] = [
    "BP_GradBigCrate_C",
    "BP_AirCrate_C",
    "BP_WoodenBoxResource_C",
    "BP_DestroyedStorageBag_C",
];
const ARRIVAL_CM: f64 = 175.0;
const SAMPLE_CM: f64 = 100.0;
const METAL_DOOR_APPROACH_CM: f64 = 200.0;
const EXPEDITION_ENTRY_DISTANCE_CM: f64 = 1_000.0;
const MOVE_TIMEOUT: Duration = Duration::from_secs(90);
const ENTRY_TIMEOUT: Duration = Duration::from_secs(15);

struct StopMovement<'a> {
    api: &'a Api,
    controller_selector: String,
}

impl Drop for StopMovement<'_> {
    fn drop(&mut self) {
        let _ = self.api.try_op(
            "call",
            json!({
                "class": "Controller",
                "function": "StopMovement",
                "instance_selector": self.controller_selector,
                "parms_hex": "",
            }),
        );
    }
}

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

fn position(point: [f64; 3]) -> Position {
    Position::new(point[0], point[1], point[2])
}

fn point(position: Position) -> [f64; 3] {
    [position.x, position.y, position.z]
}

fn spawn_to_expedition_route(
    spawn: [f64; 3],
    metal_door: [f64; 3],
    expedition_door: [f64; 3],
) -> RouteGraph {
    RouteGraph::new(
        "misery spawn to expedition",
        vec![
            Waypoint::new("spawn", position(spawn), ARRIVAL_CM),
            Waypoint::new("metal-door", position(metal_door), ARRIVAL_CM),
            Waypoint::new("expedition-door", position(expedition_door), ARRIVAL_CM),
        ],
        vec![
            RouteEdge::new(
                "spawn->metal-door",
                "spawn",
                "metal-door",
                distance(spawn, metal_door),
            ),
            RouteEdge::new(
                "metal-door->expedition-door",
                "metal-door",
                "expedition-door",
                distance(metal_door, expedition_door),
            ),
        ],
    )
    .unwrap()
}

fn metal_door_approach(spawn: [f64; 3], door: [f64; 3]) -> [f64; 3] {
    let dx = spawn[0] - door[0];
    let dy = spawn[1] - door[1];
    let length = (dx * dx + dy * dy).sqrt();
    assert!(length > 0.0, "spawn and metal door share one position");
    [
        door[0] + dx / length * METAL_DOOR_APPROACH_CM,
        door[1] + dy / length * METAL_DOOR_APPROACH_CM,
        door[2],
    ]
}

fn retain_breadcrumb(breadcrumbs: &mut Vec<Position>, current: Position) {
    if breadcrumbs
        .last()
        .is_none_or(|last| last.distance(current) >= SAMPLE_CM)
    {
        breadcrumbs.push(current);
    }
}

fn expedition_entry_observed(expedition_door: [f64; 3], player: [f64; 3]) -> bool {
    distance(expedition_door, player) >= EXPEDITION_ENTRY_DISTANCE_CM
}

fn game_viewport(api: &Api) -> isize {
    let own = api.op("input.self.hwnd", json!({}));
    assert!(own.ok, "game hwnd failed: {:?}", own.error);
    let own_hwnd = isize::from_str_radix(
        own.result["hwnd"]
            .as_str()
            .expect("window handle is a string")
            .trim_start_matches("0x"),
        16,
    )
    .expect("window handle is hex");
    let pid = modforge::input::window_pid(own_hwnd).expect("game window has an owning process");
    modforge::input::find_hwnd_by_pid(pid).expect("game viewport exists")
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

fn function_layout(api: &Api, class: &str, function: &str) -> Value {
    let layout = api.op(
        "function_parameters",
        json!({"class": class, "function": function}),
    );
    assert!(
        layout.ok,
        "{class}::{function} layout failed: {:?}",
        layout.error
    );
    println!("navigation_function_layout={}", layout.result);
    layout.result
}

fn parameter_offset(layout: &Value, name: &str) -> usize {
    layout["parameters"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|parameter| parameter["name"] == name)
        .and_then(|parameter| parameter["offset"].as_u64())
        .unwrap_or_else(|| panic!("function has no {name} parameter")) as usize
}

fn try_project_to_navigation(api: &Api, world_context: u64, point: [f64; 3]) -> Option<[f64; 3]> {
    let layout = function_layout(api, "NavigationSystemV1", "K2_ProjectPointToNavigation");
    let cdo = client::find_class_cdo(api, "NavigationSystemV1")
        .expect("NavigationSystemV1 CDO is not loaded");
    let mut parms = vec![0u8; layout["parms_size"].as_u64().unwrap() as usize];
    put_u64(
        &mut parms,
        parameter_offset(&layout, "WorldContextObject"),
        world_context,
    );
    let point_offset = parameter_offset(&layout, "Point");
    for (index, value) in point.into_iter().enumerate() {
        put_f64(&mut parms, point_offset + index * 8, value);
    }
    let extent_offset = parameter_offset(&layout, "QueryExtent");
    for (index, value) in [250.0, 250.0, 250.0].into_iter().enumerate() {
        put_f64(&mut parms, extent_offset + index * 8, value);
    }
    let (out, _) = api
        .call_ufunction(
            "NavigationSystemV1",
            "K2_ProjectPointToNavigation",
            &cdo.addr_selector,
            &parms,
        )
        .expect("K2_ProjectPointToNavigation failed");
    if out[parameter_offset(&layout, "ReturnValue")] == 0 {
        return None;
    }
    let projected = parameter_offset(&layout, "ProjectedLocation");
    Some([
        client::from_le_f64(&out, projected),
        client::from_le_f64(&out, projected + 8),
        client::from_le_f64(&out, projected + 16),
    ])
}

fn wait_for_navigation(api: &Api, world_context: u64, point: [f64; 3]) -> [f64; 3] {
    let started = Instant::now();
    loop {
        if let Some(projected) = try_project_to_navigation(api, world_context, point) {
            return projected;
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "Unreal navigation did not become ready around {point:?}"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn player_controller(api: &Api, player_selector: &str) -> u64 {
    let layout = function_layout(api, "Pawn", "GetController");
    let parms = vec![0u8; layout["parms_size"].as_u64().unwrap() as usize];
    let (out, _) = api
        .call_ufunction("Pawn", "GetController", player_selector, &parms)
        .expect("Pawn::GetController failed");
    client::from_le_u64(&out, parameter_offset(&layout, "ReturnValue"))
}

fn start_simple_move(api: &Api, controller: u64, goal: [f64; 3]) {
    let layout = function_layout(api, "AIBlueprintHelperLibrary", "SimpleMoveToLocation");
    let cdo = client::find_class_cdo(api, "AIBlueprintHelperLibrary")
        .expect("AIBlueprintHelperLibrary CDO is not loaded");
    let mut parms = vec![0u8; layout["parms_size"].as_u64().unwrap() as usize];
    put_u64(
        &mut parms,
        parameter_offset(&layout, "Controller"),
        controller,
    );
    let goal_offset = parameter_offset(&layout, "Goal");
    for (index, value) in goal.into_iter().enumerate() {
        put_f64(&mut parms, goal_offset + index * 8, value);
    }
    api.call_ufunction(
        "AIBlueprintHelperLibrary",
        "SimpleMoveToLocation",
        &cdo.addr_selector,
        &parms,
    )
    .expect("SimpleMoveToLocation failed");
}

fn stop_movement(api: &Api, controller: u64) {
    api.call_ufunction(
        "Controller",
        "StopMovement",
        &format!("addr:0x{controller:X}"),
        &[],
    )
    .expect("Controller::StopMovement failed");
}

fn print_nearby_navigation_actors(api: &Api, location: [f64; 3]) {
    let mut seen = HashSet::new();
    for needle in ["Door", "Gate", "Ladder", "Elevator", "Stair", "Lift"] {
        for actor in client::walk_class_chain_instances(api, needle, 256) {
            if !seen.insert(actor.addr) {
                continue;
            }
            let Some(suffix) = actor.full_name.split(".PersistentLevel.").nth(1) else {
                continue;
            };
            if suffix.contains('.') {
                continue;
            }
            let candidate = actor_location(api, &actor.addr_selector);
            let nearby = distance(location, candidate);
            if nearby <= 1_000.0 {
                println!(
                    "navigation_obstacle_candidate={} location={candidate:?} distance={nearby:.1}",
                    actor.full_name
                );
            }
        }
    }
}

fn interact(api: &Api) {
    let viewport = game_viewport(api);
    let response = api.op(
        "input.key.press",
        json!({
            "key": "e",
            "hold_ms": 80,
            "backend": "l2",
            "hwnd": format!("0x{:X}", viewport as usize),
        }),
    );
    assert!(
        response.ok,
        "interaction keypress failed: {:?}",
        response.error
    );
}

fn walk_edge(
    api: &Api,
    player_selector: &str,
    controller: u64,
    waypoint: &Waypoint,
    bunker_door: Option<[f64; 3]>,
    breadcrumbs: &mut Vec<Position>,
) -> usize {
    start_simple_move(api, controller, point(waypoint.position));
    let started = Instant::now();
    let mut stuck = StuckDetector::new(50.0, 10_000).unwrap();
    let mut last_report = 0;
    let mut interactions = 0;
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let current = actor_location(api, player_selector);
        retain_breadcrumb(breadcrumbs, position(current));
        let remaining = distance(current, point(waypoint.position));
        if remaining <= waypoint.arrival_radius {
            return interactions;
        }
        let elapsed_ms = started.elapsed().as_millis() as u64;
        assert!(
            started.elapsed() < MOVE_TIMEOUT,
            "Unreal navigation timed out at {current:?}, {remaining:.1} cm from '{}'",
            waypoint.id
        );
        if stuck.observe(elapsed_ms, remaining) {
            print_nearby_navigation_actors(api, current);
            let door_is_blocking = bunker_door
                .is_some_and(|door| distance(current, door) <= 1_000.0 && interactions < 3);
            assert!(
                door_is_blocking,
                "Unreal navigation stuck at {current:?}, {remaining:.1} cm from '{}'",
                waypoint.id
            );
            interactions += 1;
            println!("bunker_door_interact attempt={interactions} location={current:?}");
            interact(api);
            std::thread::sleep(Duration::from_secs(1));
            start_simple_move(api, controller, point(waypoint.position));
            stuck = StuckDetector::new(50.0, 10_000).unwrap();
        }
        let elapsed_seconds = started.elapsed().as_secs();
        if elapsed_seconds > last_report {
            println!(
                "navigation_progress waypoint={} elapsed_s={elapsed_seconds} location={current:?} remaining_cm={remaining:.1}",
                waypoint.id
            );
            last_report = elapsed_seconds;
        }
    }
}

fn enter_expedition(api: &Api, expedition_door: [f64; 3]) -> ([f64; 3], usize) {
    for attempt in 1..=3 {
        println!("expedition_door_interact attempt={attempt}");
        interact(api);
        let started = Instant::now();
        loop {
            if let Some(player) = client::find_live_instance(api, PLAYER_CLASS) {
                let current = actor_location(api, &player.addr_selector);
                if expedition_entry_observed(expedition_door, current) {
                    return (current, attempt);
                }
            }
            if started.elapsed() >= ENTRY_TIMEOUT {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
    panic!("E was pressed at the expedition door, but the player did not enter the expedition")
}

#[test]
#[ignore = "reads the live MISERY player's interaction functions"]
fn player_interaction_surface_is_discoverable() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");
    let surface = matching_functions(&api, PLAYER_CLASS, &["Interact", "InpActEvt_E", "Use"]);
    println!("player_interaction_surface={surface}");
    assert!(
        surface["functions"]
            .as_array()
            .is_some_and(|functions| !functions.is_empty()),
        "the live player exposes no interaction function"
    );
    let layout = function_layout(
        &api,
        PLAYER_CLASS,
        "InpActEvt_InteractInput_K2Node_EnhancedInputActionEvent_0",
    );
    println!("player_interaction_layout={layout}");
    println!(
        "player_sgk_interact_layout={}",
        function_layout(&api, PLAYER_CLASS, "SGK Interact")
    );
}

#[test]
#[ignore = "discovers placed loot boxes in a live MISERY expedition"]
fn nearby_expedition_loot_box_is_discoverable() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");
    let player = client::find_live_instance(&api, PLAYER_CLASS)
        .expect("enter an expedition so the live player exists");
    let player_location = actor_location(&api, &player.addr_selector);
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    for class in LOOT_BOX_CLASSES {
        for actor in client::walk_class_chain_instances(&api, class, 256) {
            if !seen.insert(actor.addr) {
                continue;
            }
            let location = actor_location(&api, &actor.addr_selector);
            let nearby = distance(player_location, location);
            println!(
                "loot_box_candidate={} location={location:?} distance={nearby:.1}",
                actor.full_name
            );
            candidates.push((nearby, actor, location));
        }
    }
    let nearest = candidates
        .into_iter()
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .expect("the live expedition has no known placed loot box");
    println!(
        "nearest_loot_box={} location={:?} distance={:.1}",
        nearest.1.full_name, nearest.2, nearest.0
    );
}

#[test]
#[ignore = "moves the live MISERY player into an expedition"]
fn unreal_navigation_enters_expedition_through_three_stops() {
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
    if distance(start, goal) <= ARRIVAL_CM {
        let started = Instant::now();
        let (entered_location, expedition_door_interactions) = enter_expedition(&api, goal);
        println!(
            "expedition_entered_from_existing_stop elapsed_s={:.2} location={entered_location:?} expedition_door_interactions={expedition_door_interactions}",
            started.elapsed().as_secs_f64(),
        );
        return;
    }
    let metal_door = client::walk_class_chain_instances(&api, METAL_DOOR_CLASS, 32)
        .into_iter()
        .filter_map(|candidate| {
            let location = actor_location(&api, &candidate.addr_selector);
            let from_spawn = distance(start, location);
            let from_expedition = distance(goal, location);
            println!(
                "metal_door_candidate={} location={location:?} from_spawn={from_spawn:.1} from_expedition={from_expedition:.1}",
                candidate.full_name
            );
            (from_spawn >= 100.0).then_some((from_spawn, candidate, location))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .expect("the bunker metal door is not loaded");

    println!("player={} location={start:?}", player.full_name);
    println!(
        "metal_door={} location={:?}",
        metal_door.1.full_name, metal_door.2
    );
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
        ("AIBlueprintHelperLibrary", &["SimpleMove"][..]),
        (PLAYER_CLASS, &["Interact", "InpActEvt_E", "Use"][..]),
    ] {
        println!(
            "navigation_surface={}",
            matching_functions(&api, class, needles)
        );
    }

    let projected_start = wait_for_navigation(&api, player.addr, start);
    let projected_metal_door = wait_for_navigation(&api, player.addr, metal_door.2);
    let metal_door_stop = wait_for_navigation(
        &api,
        player.addr,
        metal_door_approach(projected_start, projected_metal_door),
    );
    let projected_goal = wait_for_navigation(&api, player.addr, goal);
    println!(
        "projected_start={projected_start:?} metal_door_stop={metal_door_stop:?} projected_goal={projected_goal:?}"
    );

    let route = spawn_to_expedition_route(projected_start, metal_door_stop, projected_goal);
    let route_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join("misery-routes")
        .join("spawn-to-expedition.json");
    std::fs::create_dir_all(route_path.parent().unwrap()).unwrap();
    std::fs::write(&route_path, route.to_json().unwrap()).unwrap();
    let route = RouteGraph::from_json(&std::fs::read_to_string(&route_path).unwrap()).unwrap();
    assert_eq!(route.waypoints().len(), 3);
    assert_eq!(route.edges().len(), 2);
    assert_eq!(
        route
            .shortest_path("spawn", "expedition-door", |_| true)
            .unwrap(),
        ["spawn", "metal-door", "expedition-door"]
    );
    println!("route_saved={}", route_path.display());

    let controller = player_controller(&api, &player.addr_selector);
    assert_ne!(controller, 0, "the live player has no controller");
    let controller_selector = format!("addr:0x{controller:X}");
    let _stop = StopMovement {
        api: &api,
        controller_selector,
    };
    let started = Instant::now();
    let mut breadcrumbs = vec![position(start)];
    let metal_door_waypoint = route.waypoint("metal-door").unwrap();
    let expedition_door_waypoint = route.waypoint("expedition-door").unwrap();
    let first_edge_interactions = walk_edge(
        &api,
        &player.addr_selector,
        controller,
        metal_door_waypoint,
        None,
        &mut breadcrumbs,
    );
    assert_eq!(first_edge_interactions, 0);
    let bunker_door_interactions = walk_edge(
        &api,
        &player.addr_selector,
        controller,
        expedition_door_waypoint,
        Some(projected_metal_door),
        &mut breadcrumbs,
    );
    stop_movement(&api, controller);
    let (entered_location, expedition_door_interactions) = enter_expedition(&api, projected_goal);
    println!(
        "expedition_entered elapsed_s={:.2} location={entered_location:?} waypoints={} edges={} breadcrumbs={} bunker_door_interactions={bunker_door_interactions} expedition_door_interactions={expedition_door_interactions}",
        started.elapsed().as_secs_f64(),
        route.waypoints().len(),
        route.edges().len(),
        breadcrumbs.len(),
    );
}

#[test]
fn spawn_to_expedition_route_contains_only_meaningful_stops() {
    let route = spawn_to_expedition_route([0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [200.0, 0.0, 0.0]);

    assert_eq!(
        route
            .waypoints()
            .iter()
            .map(|waypoint| waypoint.id.as_str())
            .collect::<Vec<_>>(),
        ["spawn", "metal-door", "expedition-door"]
    );
    assert_eq!(route.edges().len(), 2);
}

#[test]
fn expedition_entry_requires_the_player_to_leave_the_safe_area_door() {
    let door = [100.0, 100.0, 0.0];

    assert!(!expedition_entry_observed(door, [150.0, 100.0, 0.0]));
    assert!(expedition_entry_observed(door, [2_000.0, 100.0, 0.0]));
}
