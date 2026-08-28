//! Walk from the live MISERY player into an expedition through Unreal navigation.
//!
//! Unreal owns the navigation mesh and supplies path points between three
//! meaningful stops. Modforge's shared bot travels through those points using
//! the same movement, look, and interaction inputs as the player.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 k3sc cargo-lock test -p misery-mod \
//!   --test research_navigation -- --ignored --test-threads=1 --nocapture
//! ```

mod common;

use std::time::{Duration, Instant};

use common::{Api, api_or_skip, offsets_live};
use modforge::client::{self, ClassInstance};
use modforge::input::{Button, InputSurface, Key, PlayerCommand};
use modforge::route::{
    Bot, BotStatus, Path, PathPoint, PlayerObservation, Position, Route, SteeringConfig, Waypoint,
};
use serde_json::{Value, json};

const METAL_DOOR_CLASS: &str = "BP_MetalDoor_C";
const EXPEDITION_DOOR_CLASS: &str = "BP_ExpeditionDoor_C";
const STORAGE_CLASS: &str = "BP_MasterStorageBuildPart_C";
const LOOT_BOX_CLASSES: [&str; 7] = [
    "BP_BigCrate_C",
    "BP_MidCrate_C",
    "BP_WoodenCrate2_C",
    "BP_MilBigCrateWorld_C",
    "BP_MilMidCrateWorld_C",
    "BP_StashMid_C",
    "BP_Safe_C",
];
const ARRIVAL_CM: f64 = 175.0;
const METAL_DOOR_APPROACH_CM: f64 = 200.0;
const EXPEDITION_ENTRY_DISTANCE_CM: f64 = 1_000.0;
const MOVE_TIMEOUT: Duration = Duration::from_secs(90);
const ENTRY_TIMEOUT: Duration = Duration::from_secs(15);
const INTERACTION_RANGE_CM: f64 = 300.0;
const INVENTORY_ITEM_COUNT_OFFSET: u64 = 0xC0;
const INVENTORY_USING_PLAYERS_NUM_OFFSET: u64 = 0xB0;
const PLAYER_CHARACTER_COMPONENT_OFFSET: u64 = 0x740;
const CHARACTER_PLAYER_INVENTORY_OFFSET: u64 = 0x218;

struct NavigationCalls {
    project: Value,
    find_path: Value,
    path_points_offset: usize,
}

struct ReachableLootBox {
    actor: ClassInstance,
    target: [f64; 3],
    path_cost: f64,
}

impl NavigationCalls {
    fn new(api: &Api) -> Self {
        Self {
            project: function_layout(api, "NavigationSystemV1", "K2_ProjectPointToNavigation"),
            find_path: function_layout(
                api,
                "NavigationSystemV1",
                "FindPathToLocationSynchronously",
            ),
            path_points_offset: class_field_offset(api, "NavigationPath", "PathPoints"),
        }
    }
}

struct ControlPlaneInput<'a> {
    api: &'a Api,
}

impl InputSurface for ControlPlaneInput<'_> {
    fn name(&self) -> &'static str {
        "misery-control-plane"
    }

    fn click(&self, _button: Button, _x: i32, _y: i32) -> Result<(), String> {
        Err("MISERY route input does not use absolute UI clicks".into())
    }

    fn move_abs(&self, _x: i32, _y: i32) -> Result<(), String> {
        Err("MISERY route input does not move the physical cursor".into())
    }

    fn move_rel(&self, dx: i32, dy: i32) -> Result<(), String> {
        self.commands(&[PlayerCommand::mouse_delta(dx, dy)])
    }

    fn key(&self, key: Key, down: bool) -> Result<(), String> {
        self.commands(&[PlayerCommand::key(key, down)])
    }

    fn commands(&self, commands: &[PlayerCommand]) -> Result<(), String> {
        let response = self
            .api
            .try_op("input.player.commands", json!({"commands": commands}))
            .map_err(|error| format!("player command request failed: {error}"))?;
        if response.ok {
            Ok(())
        } else {
            Err(response
                .error
                .unwrap_or_else(|| "player command batch failed".into()))
        }
    }

    fn observe_player(&self) -> Result<PlayerObservation, String> {
        let response = self
            .api
            .try_op("input.player.pose", json!({}))
            .map_err(|error| format!("player pose request failed: {error}"))?;
        if !response.ok {
            return Err(response
                .error
                .unwrap_or_else(|| "player pose observation failed".into()));
        }
        serde_json::from_value(response.result["pose"].clone())
            .map_err(|error| format!("decode player observation: {error}"))
    }
}

struct StopMovement<'a> {
    api: &'a Api,
}

struct TimingGuard<'a> {
    api: &'a Api,
    active: bool,
}

impl<'a> TimingGuard<'a> {
    fn start(api: &'a Api) -> Self {
        let response = api.op("timing", json!({"on": true, "reset": true}));
        assert!(response.ok, "could not start timing: {:?}", response.error);
        Self { api, active: true }
    }

    fn finish(mut self) -> Value {
        let report = self.api.op("timing_report", json!({}));
        assert!(report.ok, "could not read timing: {:?}", report.error);
        assert_performance_report(&report.result);
        self.disable()
            .unwrap_or_else(|error| panic!("could not stop timing: {error}"));
        report.result
    }

    fn disable(&mut self) -> Result<(), String> {
        if !self.active {
            return Ok(());
        }
        let response = self
            .api
            .try_op("timing", json!({"on": false, "reset": false}))
            .map_err(|error| format!("timing request failed: {error}"))?;
        if !response.ok {
            return Err(response.error.unwrap_or_else(|| "unknown error".into()));
        }
        self.active = false;
        Ok(())
    }
}

impl Drop for TimingGuard<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.disable() {
            eprintln!("could not disable route timing during cleanup: {error}");
        }
    }
}

fn assert_performance_report(report: &Value) {
    let entries = report["entries"]
        .as_array()
        .expect("timing report has entries");
    for forbidden in [
        "ue:find_object",
        "ue:find_objects_by_chain",
        "ue:find_actors_by_chain",
        "ue:objects_read",
    ] {
        assert!(
            entries.iter().all(|entry| entry["name"] != forbidden),
            "route performed forbidden global object work: {forbidden}"
        );
    }
    for entry in entries.iter().filter(|entry| {
        entry["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("ue:"))
    }) {
        assert!(
            entry["worst_ms"].as_f64().unwrap_or(f64::INFINITY) < 16.7,
            "Unreal bot operation consumed a full frame: {entry}"
        );
    }
}

impl Drop for StopMovement<'_> {
    fn drop(&mut self) {
        let _ = self.api.try_op(
            "input.player.commands",
            json!({"commands": [
                PlayerCommand::key(Key(0x57), false),
                PlayerCommand::key(Key(0x41), false),
                PlayerCommand::key(Key(0x53), false),
                PlayerCommand::key(Key(0x44), false),
            ]}),
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

fn retained_player(api: &Api) -> ClassInstance {
    client::resolve_selector(api, "live_player")
        .expect("the current-world MISERY player is not retained")
}

fn scene_component_location(api: &Api, selector: &str) -> [f64; 3] {
    let (out, _) = api
        .call_ufunction(
            "SceneComponent",
            "K2_GetComponentLocation",
            selector,
            &[0u8; 0x18],
        )
        .expect("K2_GetComponentLocation failed");
    [
        client::from_le_f64(&out, 0x00),
        client::from_le_f64(&out, 0x08),
        client::from_le_f64(&out, 0x10),
    ]
}

fn pawn_view_location(api: &Api, player_selector: &str) -> [f64; 3] {
    let player = u64::from_str_radix(player_selector.trim_start_matches("addr:0x"), 16)
        .expect("player selector is an address");
    let camera = component_of_class(api, player, "CameraComponent");
    scene_component_location(api, &format!("addr:0x{camera:X}"))
}

fn actor_bounds(api: &Api, selector: &str) -> ([f64; 3], [f64; 3]) {
    let mut parms = [0u8; 57];
    parms[0] = 1;
    let (out, _) = api
        .call_ufunction("Actor", "GetActorBounds", selector, &parms)
        .expect("GetActorBounds failed");
    let vector = |offset| {
        [
            client::from_le_f64(&out, offset),
            client::from_le_f64(&out, offset + 8),
            client::from_le_f64(&out, offset + 16),
        ]
    };
    (vector(8), vector(32))
}

fn interaction_in_range(view: [f64; 3], origin: [f64; 3], extent: [f64; 3]) -> bool {
    let mut squared = 0.0;
    for axis in 0..3 {
        let outside = ((view[axis] - origin[axis]).abs() - extent[axis]).max(0.0);
        squared += outside * outside;
    }
    squared.sqrt() <= INTERACTION_RANGE_CM
}

fn interaction_allowed(api: &Api, target: &ClassInstance) -> bool {
    let class = target
        .full_name
        .split_whitespace()
        .next()
        .unwrap_or_default();
    api.call_ufunction(class, "SGK AllowInteraction", &target.addr_selector, &[0])
        .is_ok_and(|(out, _)| out.first() == Some(&1))
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
) -> Route {
    Route::new(vec![
        Waypoint::new("spawn", position(spawn), ARRIVAL_CM),
        Waypoint::new("metal-door", position(metal_door), ARRIVAL_CM),
        Waypoint::new("expedition-door", position(expedition_door), ARRIVAL_CM),
    ])
    .unwrap()
}

fn expedition_to_loot_route(start: [f64; 3], loot_box: [f64; 3]) -> Route {
    Route::new(vec![
        Waypoint::new("expedition-entry", position(start), ARRIVAL_CM),
        Waypoint::new("loot-box", position(loot_box), ARRIVAL_CM),
    ])
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

fn expedition_entry_observed(expedition_door: [f64; 3], player: [f64; 3]) -> bool {
    distance(expedition_door, player) >= EXPEDITION_ENTRY_DISTANCE_CM
}

fn control_rotation(api: &Api, player_selector: &str) -> [f64; 3] {
    let (out, _) = api
        .call_ufunction("Pawn", "GetControlRotation", player_selector, &[0u8; 0x18])
        .expect("GetControlRotation failed");
    [
        client::from_le_f64(&out, 0x00),
        client::from_le_f64(&out, 0x08),
        client::from_le_f64(&out, 0x10),
    ]
}

fn signed_degrees(degrees: f64) -> f64 {
    (degrees + 180.0).rem_euclid(360.0) - 180.0
}

fn look_command(yaw_error: f64, pitch_error: f64) -> (i32, i32) {
    (
        (yaw_error * 0.25).round().clamp(-10.0, 10.0) as i32,
        (-pitch_error * 0.25).round().clamp(-10.0, 10.0) as i32,
    )
}

fn player_commands(api: &Api, commands: &[PlayerCommand]) {
    let response = api.op("input.player.commands", json!({"commands": commands}));
    assert!(response.ok, "player commands failed: {:?}", response.error);
}

fn aim_at(api: &Api, player_selector: &str, target: [f64; 3]) {
    let started = Instant::now();
    let mut unchanged_steps = 0;
    loop {
        let view = pawn_view_location(api, player_selector);
        let rotation = control_rotation(api, player_selector);
        let dx = target[0] - view[0];
        let dy = target[1] - view[1];
        let horizontal = (dx * dx + dy * dy).sqrt();
        let target_yaw = dy.atan2(dx).to_degrees();
        let target_pitch = -(target[2] - view[2]).atan2(horizontal).to_degrees();
        let yaw_error = signed_degrees(target_yaw - rotation[1]);
        let pitch_error = signed_degrees(target_pitch - signed_degrees(rotation[0]));
        if yaw_error.abs() <= 1.0 && pitch_error.abs() <= 1.0 {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "could not aim at target: rotation={rotation:?} target_yaw={target_yaw:.1} target_pitch={target_pitch:.1} error=({yaw_error:.1},{pitch_error:.1})"
        );
        let (mouse_x, mouse_y) = look_command(yaw_error, pitch_error);
        player_commands(api, &[PlayerCommand::mouse_delta(mouse_x, mouse_y)]);
        std::thread::sleep(Duration::from_millis(16));
        let after = control_rotation(api, player_selector);
        if signed_degrees(after[1] - rotation[1]).abs() < 0.01
            && signed_degrees(after[0] - rotation[0]).abs() < 0.01
        {
            unchanged_steps += 1;
            assert!(
                unchanged_steps < 10,
                "player controller ignored ten consecutive look commands"
            );
        } else {
            unchanged_steps = 0;
        }
    }
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

fn class_field_offset(api: &Api, class: &str, field: &str) -> usize {
    let detail = api.op("discover_class_detail", json!({"name": class}));
    assert!(detail.ok, "{class} detail failed: {:?}", detail.error);
    let offset = reflected_field_offset(&detail.result, field)
        .unwrap_or_else(|error| panic!("{class}.{field}: {error}"));
    println!("navigation_field_offset={class}.{field}@0x{offset:X}");
    offset
}

fn reflected_field_offset(detail: &Value, field: &str) -> Result<usize, String> {
    detail["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|candidate| candidate["name"] == field)
        .and_then(|candidate| candidate["offset"].as_u64())
        .map(|offset| offset as usize)
        .ok_or_else(|| format!("reflected field '{field}' not found"))
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

fn try_project_to_navigation(
    api: &Api,
    calls: &NavigationCalls,
    world_context: u64,
    point: [f64; 3],
) -> Option<[f64; 3]> {
    let layout = &calls.project;
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
            "singleton:NavigationSystemV1",
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

fn find_navigation_path(
    api: &Api,
    calls: &NavigationCalls,
    world_context: u64,
    pathfinding_context: u64,
    start: [f64; 3],
    goal: [f64; 3],
) -> Option<(u64, f64)> {
    let layout = &calls.find_path;
    let mut parms = vec![0u8; layout["parms_size"].as_u64().unwrap() as usize];
    put_u64(
        &mut parms,
        parameter_offset(&layout, "WorldContextObject"),
        world_context,
    );
    put_u64(
        &mut parms,
        parameter_offset(&layout, "PathfindingContext"),
        pathfinding_context,
    );
    for (name, point) in [("PathStart", start), ("PathEnd", goal)] {
        let offset = parameter_offset(&layout, name);
        for (index, value) in point.into_iter().enumerate() {
            put_f64(&mut parms, offset + index * 8, value);
        }
    }
    let (out, _) = api
        .call_ufunction(
            "NavigationSystemV1",
            "FindPathToLocationSynchronously",
            "singleton:NavigationSystemV1",
            &parms,
        )
        .expect("FindPathToLocationSynchronously failed");
    let path = client::from_le_u64(&out, parameter_offset(&layout, "ReturnValue"));
    if path == 0 {
        return None;
    }
    let selector = format!("addr:0x{path:X}");
    let (valid, _) = api
        .call_ufunction("NavigationPath", "IsValid", &selector, &[0])
        .expect("NavigationPath::IsValid failed");
    let (partial, _) = api
        .call_ufunction("NavigationPath", "IsPartial", &selector, &[0])
        .expect("NavigationPath::IsPartial failed");
    if valid[0] == 0 || partial[0] != 0 {
        return None;
    }
    let (length, _) = api
        .call_ufunction("NavigationPath", "GetPathLength", &selector, &[0; 8])
        .expect("NavigationPath::GetPathLength failed");
    let cost = client::from_le_f64(&length, 0);
    (cost.is_finite() && cost > 0.0).then_some((path, cost))
}

fn navigation_path_cost(
    api: &Api,
    calls: &NavigationCalls,
    world_context: u64,
    pathfinding_context: u64,
    start: [f64; 3],
    goal: [f64; 3],
) -> Option<f64> {
    let goal = try_project_to_navigation(api, calls, world_context, goal)?;
    find_navigation_path(api, calls, world_context, pathfinding_context, start, goal)
        .map(|(_, cost)| cost)
}

fn navigation_path_points(
    api: &Api,
    calls: &NavigationCalls,
    world_context: u64,
    pathfinding_context: u64,
    start: [f64; 3],
    goal: [f64; 3],
) -> Option<Vec<[f64; 3]>> {
    let goal = try_project_to_navigation(api, calls, world_context, goal)?;
    let (path, _) =
        find_navigation_path(api, calls, world_context, pathfinding_context, start, goal)?;
    let header = client::read_bytes(api, path, calls.path_points_offset as u64, 16);
    let data = client::from_le_u64(&header, 0);
    let count = client::from_le_i32(&header, 8);
    let capacity = client::from_le_i32(&header, 12);
    if data == 0 || count < 2 || capacity < count || count > 4_096 {
        return None;
    }
    let bytes = client::read_bytes(api, data, 0, count as u64 * 24);
    decode_path_points(&bytes, count as usize).ok()
}

fn decode_path_points(bytes: &[u8], count: usize) -> Result<Vec<[f64; 3]>, String> {
    let required = count
        .checked_mul(24)
        .ok_or_else(|| "navigation path point count overflow".to_string())?;
    if bytes.len() < required {
        return Err(format!(
            "navigation path needs {required} bytes for {count} points, got {}",
            bytes.len()
        ));
    }
    Ok((0..count)
        .map(|index| {
            let offset = index * 24;
            [
                client::from_le_f64(bytes, offset),
                client::from_le_f64(bytes, offset + 8),
                client::from_le_f64(bytes, offset + 16),
            ]
        })
        .collect())
}

fn wait_for_navigation(
    api: &Api,
    calls: &NavigationCalls,
    world_context: u64,
    point: [f64; 3],
) -> [f64; 3] {
    let started = Instant::now();
    loop {
        if let Some(projected) = try_project_to_navigation(api, calls, world_context, point) {
            return projected;
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "Unreal navigation did not become ready around {point:?}"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn interact(api: &Api, _calls: &NavigationCalls, _player_selector: &str) {
    player_commands(
        api,
        &[
            PlayerCommand::key(Key(0x45), true),
            PlayerCommand::key(Key(0x45), false),
        ],
    );
}

fn nearest_loot_box(
    api: &Api,
    world_context: u64,
    player_location: [f64; 3],
) -> (ClassInstance, [f64; 3]) {
    let candidates = loot_box_candidates(api, world_context, player_location);
    let nearest = candidates
        .into_iter()
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .expect("the live expedition has no known placed loot box");
    println!(
        "nearest_loot_box={} location={:?} distance={:.1}",
        nearest.1.full_name, nearest.2, nearest.0
    );
    (nearest.1, nearest.2)
}

fn is_loot_box(actor: &ClassInstance) -> bool {
    let class = actor
        .full_name
        .split_whitespace()
        .next()
        .unwrap_or_default();
    LOOT_BOX_CLASSES.contains(&class)
}

fn loot_box_candidates(
    api: &Api,
    world_context: u64,
    player_location: [f64; 3],
) -> Vec<(f64, ClassInstance, [f64; 3])> {
    assert_ne!(world_context, 0, "loot discovery requires a world context");
    client::actors_of_class(api, world_context, STORAGE_CLASS)
        .into_iter()
        .filter(is_loot_box)
        .filter_map(|actor| {
            let location = actor_location(api, &actor.addr_selector);
            let nearby = distance(player_location, location);
            (nearby <= 15_000.0).then_some((nearby, actor, location))
        })
        .collect()
}

fn reachable_loot_boxes(
    api: &Api,
    calls: &NavigationCalls,
    player: &ClassInstance,
    player_location: [f64; 3],
) -> Vec<ReachableLootBox> {
    let mut candidates = loot_box_candidates(api, player.addr, player_location);
    candidates.sort_by(|left, right| left.0.total_cmp(&right.0));
    let projected_start = wait_for_navigation(api, calls, player.addr, player_location);
    let mut reachable = Vec::new();
    for (nearby, actor, location) in candidates.into_iter().take(20) {
        let Some(path_cost) = navigation_path_cost(
            api,
            calls,
            player.addr,
            player.addr,
            projected_start,
            location,
        ) else {
            println!("loot_box_unreachable={}", actor.full_name);
            continue;
        };
        println!(
            "loot_box_reachable={} location={location:?} nearby={nearby:.1} path_cost={path_cost:.1}",
            actor.full_name
        );
        reachable.push(ReachableLootBox {
            actor,
            target: location,
            path_cost,
        });
    }
    reachable.sort_by(|left, right| left.path_cost.total_cmp(&right.path_cost));
    assert!(
        !reachable.is_empty(),
        "the live expedition has no reachable known loot box"
    );
    reachable
}

fn component_of_class(api: &Api, actor: u64, class: &str) -> u64 {
    let response = api.op(
        "component_of_class",
        json!({"actor": actor, "class": class}),
    );
    assert!(response.ok, "component lookup failed: {:?}", response.error);
    response.result["component"]
        .as_str()
        .and_then(|address| u64::from_str_radix(address.trim_start_matches("0x"), 16).ok())
        .unwrap_or_else(|| panic!("{class} is not attached to actor 0x{actor:X}"))
}

fn player_inventory(api: &Api, player: u64) -> u64 {
    let character = client::read_u64(api, player, PLAYER_CHARACTER_COMPONENT_OFFSET);
    assert_ne!(character, 0, "player has no character component");
    let inventory = client::read_u64(api, character, CHARACTER_PLAYER_INVENTORY_OFFSET);
    assert_ne!(inventory, 0, "character component has no player inventory");
    inventory
}

fn open_loot_box(
    api: &Api,
    calls: &NavigationCalls,
    player_selector: &str,
    storage_inventory: u64,
) -> usize {
    for attempt in 1..=3 {
        println!("loot_box_interact attempt={attempt}");
        interact(api, calls, player_selector);
        let started = Instant::now();
        while started.elapsed() < ENTRY_TIMEOUT {
            if client::read_i32(api, storage_inventory, INVENTORY_USING_PLAYERS_NUM_OFFSET) > 0 {
                return attempt;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    panic!("E was pressed at the loot box, but its inventory did not gain a using player")
}

fn open_bunker_door_at_stop(
    api: &Api,
    calls: &NavigationCalls,
    player_selector: &str,
    door: &ClassInstance,
) -> Result<bool, String> {
    if !interaction_allowed(api, door) {
        return Ok(false);
    }
    let (interaction_point, extent) = actor_bounds(api, &door.addr_selector);
    aim_at(api, player_selector, interaction_point);
    let view = pawn_view_location(api, player_selector);
    if !interaction_in_range(view, interaction_point, extent) {
        return Err(format!(
            "door interaction is out of range: view={view:?} point={interaction_point:?} extent={extent:?}"
        ));
    }
    interact(api, calls, player_selector);
    std::thread::sleep(Duration::from_secs(1));
    Ok(true)
}

fn walk_to_waypoint(
    api: &Api,
    calls: &NavigationCalls,
    world_context: u64,
    pathfinding_context: u64,
    player_selector: &str,
    waypoint: &Waypoint,
) -> Result<(), String> {
    let started = Instant::now();
    let mut last_report = 0;
    let surface = ControlPlaneInput { api };
    'replan: loop {
        let current = actor_location(api, player_selector);
        let path_points = navigation_path_points(
            api,
            calls,
            world_context,
            pathfinding_context,
            current,
            point(waypoint.position),
        )
        .ok_or_else(|| format!("Unreal returned no complete path to '{}'", waypoint.id))?;
        let path = Path::new(
            path_points
                .into_iter()
                .map(|point| PathPoint::new(position(point)))
                .collect(),
        )?;
        let steering = SteeringConfig {
            mouse_units_per_degree: 0.25,
            max_mouse_delta: 10,
            move_yaw_tolerance_deg: 10.0,
            path_point_radius: 75.0,
        };
        let mut bot = Bot::new(path, waypoint.arrival_radius, steering, 50.0, 10_000)?;

        loop {
            std::thread::sleep(Duration::from_millis(16));
            let observed = surface.observe_player()?;
            let current = point(observed.position);
            let remaining = distance(current, point(waypoint.position));
            if started.elapsed() >= MOVE_TIMEOUT {
                let output = bot.cancel();
                surface.commands(&output.commands)?;
                return Err(format!(
                    "player-input navigation timed out at {current:?}, {remaining:.1} cm from '{}'",
                    waypoint.id
                ));
            }
            let output = bot.tick(observed, started.elapsed().as_millis() as u64);
            surface.commands(&output.commands)?;
            match output.status {
                BotStatus::Arrived => return Ok(()),
                BotStatus::Travelling { .. } => {}
                BotStatus::Cancelled => {
                    return Err(format!(
                        "player-input navigation to '{}' was cancelled",
                        waypoint.id
                    ));
                }
                BotStatus::Stuck => {
                    println!(
                        "navigation_replan waypoint={} location={current:?} remaining_cm={remaining:.1}",
                        waypoint.id
                    );
                    continue 'replan;
                }
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
}

fn enter_expedition(
    api: &Api,
    calls: &NavigationCalls,
    player_selector: &str,
    expedition_door: [f64; 3],
) -> ([f64; 3], usize) {
    for attempt in 1..=3 {
        println!("expedition_door_interact attempt={attempt}");
        interact(api, calls, player_selector);
        let started = Instant::now();
        loop {
            let current = actor_location(api, player_selector);
            if expedition_entry_observed(expedition_door, current) {
                return (current, attempt);
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
#[ignore = "moves the live MISERY player with W"]
fn real_player_input_moves_on_w_and_stops_on_release() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let input = ControlPlaneInput { api: &api };
    let _stop = StopMovement { api: &api };
    let before = input.observe_player().expect("observe player before W");
    println!("w_before={:?}", before.position);

    input
        .key(Key(0x57), true)
        .expect("press W through player input");
    let started = Instant::now();
    let moving = loop {
        std::thread::sleep(Duration::from_millis(100));
        let observed = input
            .observe_player()
            .expect("observe player while W is held");
        if before.position.distance(observed.position) >= 25.0 {
            break observed;
        }
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "W did not move the player from {:?}; last position was {:?}",
            before.position,
            observed.position
        );
    };

    input
        .key(Key(0x57), false)
        .expect("release W through player input");
    let released = input
        .observe_player()
        .expect("observe player after W release");
    std::thread::sleep(Duration::from_millis(500));
    let stopped = input
        .observe_player()
        .expect("observe player after settling");
    std::thread::sleep(Duration::from_millis(500));
    let final_observation = input
        .observe_player()
        .expect("observe final player position");

    let moved_cm = before.position.distance(moving.position);
    let post_release_cm = stopped.position.distance(final_observation.position);
    println!(
        "w_positions before={:?} moving={:?} released={:?} stopped={:?} final={:?} moved_cm={moved_cm:.1} post_release_cm={post_release_cm:.1}",
        before.position,
        moving.position,
        released.position,
        stopped.position,
        final_observation.position,
    );
    assert!(moved_cm >= 25.0, "W moved the player only {moved_cm:.1} cm");
    assert!(
        post_release_cm <= 10.0,
        "player kept moving {post_release_cm:.1} cm after W release"
    );
}

#[test]
#[ignore = "discovers placed loot boxes in a live MISERY expedition"]
fn nearby_expedition_loot_box_is_discoverable() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");
    let player = retained_player(&api);
    let player_location = actor_location(&api, &player.addr_selector);
    nearest_loot_box(&api, player.addr, player_location);
}

#[test]
#[ignore = "moves to and opens the nearest loot box in a live MISERY expedition"]
fn unreal_navigation_opens_nearest_expedition_loot_box() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");
    let timing = TimingGuard::start(&api);
    let player = retained_player(&api);
    let calls = NavigationCalls::new(&api);
    let start = actor_location(&api, &player.addr_selector);
    let player_inventory = player_inventory(&api, player.addr);
    let player_items_before = client::read_i32(&api, player_inventory, INVENTORY_ITEM_COUNT_OFFSET);
    let _stop = StopMovement { api: &api };
    let started = Instant::now();
    let targets = reachable_loot_boxes(&api, &calls, &player, start);
    let mut selected = None;
    for target in targets {
        let current = actor_location(&api, &player.addr_selector);
        let projected_start = wait_for_navigation(&api, &calls, player.addr, current);
        let route = expedition_to_loot_route(projected_start, target.target);
        match walk_to_waypoint(
            &api,
            &calls,
            player.addr,
            player.addr,
            &player.addr_selector,
            route.waypoint("loot-box").unwrap(),
        ) {
            Ok(()) => {
                selected = Some((target, route));
                break;
            }
            Err(error) => {
                println!("loot_box_traversal_rejected={error}");
            }
        }
    }
    let (loot_box, route) = selected.expect("no engine-reachable loot box was traversable");

    let storage_inventory = component_of_class(&api, loot_box.actor.addr, "BP_MasterInventory_C");
    let storage_items_before =
        client::read_i32(&api, storage_inventory, INVENTORY_ITEM_COUNT_OFFSET);
    let mut live_loot_box_location = actor_location(&api, &loot_box.actor.addr_selector);
    live_loot_box_location[2] += 150.0;
    aim_at(&api, &player.addr_selector, live_loot_box_location);
    let interaction_attempts =
        open_loot_box(&api, &calls, &player.addr_selector, storage_inventory);
    let timing_report = timing.finish();
    println!(
        "loot_box_opened elapsed_s={:.2} target={} path_cost={:.1} interaction_attempts={interaction_attempts} storage_inventory=0x{storage_inventory:X} storage_items={storage_items_before} player_inventory=0x{player_inventory:X} player_items={player_items_before} waypoints={}",
        started.elapsed().as_secs_f64(),
        loot_box.actor.full_name,
        loot_box.path_cost,
        route.waypoints().len(),
    );
    println!("loot_performance={timing_report}");
}

#[test]
#[ignore = "moves the live MISERY player into an expedition"]
fn unreal_navigation_enters_expedition_through_three_stops() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let timing = TimingGuard::start(&api);
    let player = retained_player(&api);
    let calls = NavigationCalls::new(&api);
    let start = actor_location(&api, &player.addr_selector);
    let door = client::actors_of_class(&api, player.addr, EXPEDITION_DOOR_CLASS)
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
        let (entered_location, expedition_door_interactions) =
            enter_expedition(&api, &calls, &player.addr_selector, goal);
        println!(
            "expedition_entered_from_existing_stop elapsed_s={:.2} location={entered_location:?} expedition_door_interactions={expedition_door_interactions}",
            started.elapsed().as_secs_f64(),
        );
        println!("route_performance={}", timing.finish());
        return;
    }
    let metal_door = client::actors_of_class(&api, player.addr, METAL_DOOR_CLASS)
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

    let projected_start = wait_for_navigation(&api, &calls, player.addr, start);
    let projected_metal_door = wait_for_navigation(&api, &calls, player.addr, metal_door.2);
    let metal_door_stop = wait_for_navigation(
        &api,
        &calls,
        player.addr,
        metal_door_approach(projected_start, projected_metal_door),
    );
    let projected_goal = wait_for_navigation(&api, &calls, player.addr, goal);
    println!(
        "projected_start={projected_start:?} metal_door_stop={metal_door_stop:?} projected_goal={projected_goal:?}"
    );

    let route = spawn_to_expedition_route(projected_start, metal_door_stop, projected_goal);
    assert_eq!(route.waypoints().len(), 3);
    let _stop = StopMovement { api: &api };
    let started = Instant::now();
    let metal_door_waypoint = route.waypoint("metal-door").unwrap();
    let expedition_door_waypoint = route.waypoint("expedition-door").unwrap();
    walk_to_waypoint(
        &api,
        &calls,
        player.addr,
        player.addr,
        &player.addr_selector,
        metal_door_waypoint,
    )
    .expect("spawn to metal door traversal failed");
    let bunker_door_opened =
        open_bunker_door_at_stop(&api, &calls, &player.addr_selector, &metal_door.1)
            .expect("bunker door interaction failed");
    walk_to_waypoint(
        &api,
        &calls,
        player.addr,
        player.addr,
        &player.addr_selector,
        expedition_door_waypoint,
    )
    .expect("metal door to expedition door traversal failed");
    let (entered_location, expedition_door_interactions) =
        enter_expedition(&api, &calls, &player.addr_selector, projected_goal);
    println!(
        "expedition_entered elapsed_s={:.2} location={entered_location:?} waypoints={} bunker_door_opened={bunker_door_opened} expedition_door_interactions={expedition_door_interactions}",
        started.elapsed().as_secs_f64(),
        route.waypoints().len(),
    );
    println!("route_performance={}", timing.finish());
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
}

#[test]
fn expedition_entry_requires_the_player_to_leave_the_safe_area_door() {
    let door = [100.0, 100.0, 0.0];

    assert!(!expedition_entry_observed(door, [150.0, 100.0, 0.0]));
    assert!(expedition_entry_observed(door, [2_000.0, 100.0, 0.0]));
}

#[test]
fn expedition_loot_route_adds_only_the_discovered_target() {
    let loot_box = [200.0, 100.0, 25.0];
    let route = expedition_to_loot_route([0.0, 0.0, 0.0], loot_box);

    assert_eq!(
        route
            .waypoints()
            .iter()
            .map(|waypoint| waypoint.id.as_str())
            .collect::<Vec<_>>(),
        ["expedition-entry", "loot-box"]
    );
    assert_eq!(
        route.waypoint("loot-box").unwrap().position,
        position(loot_box)
    );
}

#[test]
fn reflected_navigation_path_field_resolves_by_name() {
    let detail = json!({
        "fields": [
            {"name": "PathPoints", "offset": 48, "element_size": 16},
            {"name": "RecalculateOnInvalidation", "offset": 64, "element_size": 1}
        ]
    });

    assert_eq!(reflected_field_offset(&detail, "PathPoints"), Ok(48));
    assert_eq!(
        reflected_field_offset(&detail, "Missing"),
        Err("reflected field 'Missing' not found".to_string())
    );
}

#[test]
fn interaction_range_uses_the_nearest_point_on_actor_bounds() {
    assert!(interaction_in_range(
        [0.0, 0.0, 0.0],
        [250.0, 0.0, 0.0],
        [100.0, 50.0, 50.0]
    ));
    assert!(!interaction_in_range(
        [0.0, 0.0, 0.0],
        [500.0, 0.0, 0.0],
        [100.0, 50.0, 50.0]
    ));
}

#[test]
fn look_commands_are_bounded_and_follow_control_rotation_errors() {
    assert_eq!(look_command(20.0, -8.0), (5, 2));
    assert_eq!(look_command(100.0, -100.0), (10, 10));
    assert_eq!(look_command(-100.0, 100.0), (-10, -10));
}

#[test]
fn navigation_path_points_decode_unreal_fvectors() {
    let mut bytes = Vec::new();
    for point in [[1.0, 2.0, 3.0], [40.0, 50.0, 60.0]] {
        for value in point {
            bytes.extend_from_slice(&f64::to_le_bytes(value));
        }
    }
    assert_eq!(
        decode_path_points(&bytes, 2).unwrap(),
        vec![[1.0, 2.0, 3.0], [40.0, 50.0, 60.0]]
    );
    assert!(decode_path_points(&bytes[..24], 2).is_err());
}

#[test]
fn performance_report_accepts_zero_scan_sub_frame_operations() {
    assert_performance_report(&json!({
        "entries": [
            {"name": "ue:actors_of_class", "worst_ms": 2.5},
            {"name": "ue:component_by_class", "worst_ms": 0.2}
        ]
    }));
}

#[test]
#[should_panic(expected = "route performed forbidden global object work")]
fn performance_report_rejects_global_object_scans() {
    assert_performance_report(&json!({
        "entries": [{"name": "ue:find_object", "worst_ms": 20.0}]
    }));
}

#[test]
#[should_panic(expected = "Unreal bot operation consumed a full frame")]
fn performance_report_rejects_full_frame_operations() {
    assert_performance_report(&json!({
        "entries": [{"name": "ue:actors_of_class", "worst_ms": 16.7}]
    }));
}
