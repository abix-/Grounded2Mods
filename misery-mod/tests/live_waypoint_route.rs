//! Record and replay a short waypoint route against the live MISERY player.
//!
//! The test focuses the game viewport, drives held forward and relative mouse
//! input, then returns to its starting position and facing.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 k3sc cargo-lock test -p misery-mod \
//!   --test live_waypoint_route -- --ignored --test-threads=1 --nocapture
//! ```

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::{Api, api_or_skip, offsets_live};
use modforge::client;
use modforge::client::live_journal::{LiveJournal, Observation, OpExecutor, RecordedOp, Recorder};
use modforge::envelope::OpResponse;
use modforge::route::{
    Pose, Position, RouteGraph, SteeringConfig, StuckDetector, TrailRecorder, Waypoint, steer,
    steer_yaw,
};
use serde_json::json;

const PLAYER_CHAIN: &str = "BP_SGKMasterCharacter_C";
const OUTBOUND_CM: f64 = 100.0;
const ARRIVAL_CM: f64 = 30.0;
const SAMPLE_CM: f64 = 25.0;
const WAYPOINT_TIMEOUT: Duration = Duration::from_secs(8);
const HEADING_OFFSETS_DEG: [f64; 4] = [0.0, 90.0, -90.0, 180.0];

struct ReleaseForward<'a>(&'a Api);

impl Drop for ReleaseForward<'_> {
    fn drop(&mut self) {
        let _ = self
            .0
            .try_op("input.key.up", json!({"key": "w", "backend": "l1"}));
    }
}

struct RemoveFile(PathBuf);

impl Drop for RemoveFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn call_vec3(api: &Api, selector: &str, class: &str, function: &str) -> (f64, f64, f64) {
    let (out, _) = api
        .call_ufunction(class, function, selector, &[0u8; 0x18])
        .unwrap_or_else(|error| panic!("{function} failed: {error}"));
    assert_eq!(out.len(), 0x18, "{function} returned the wrong parm size");
    (
        client::from_le_f64(&out, 0x00),
        client::from_le_f64(&out, 0x08),
        client::from_le_f64(&out, 0x10),
    )
}

fn pose(api: &Api, selector: &str) -> Pose {
    let (x, y, z) = call_vec3(api, selector, "Actor", "K2_GetActorLocation");
    let (_, yaw, _) = call_vec3(api, selector, "Pawn", "GetControlRotation");
    Pose::new(Position::new(x, y, z), yaw)
}

fn focus_game(api: &Api) {
    let own = api.op("input.self.hwnd", json!({}));
    assert!(own.ok, "game hwnd failed: {:?}", own.error);
    let console_hwnd = isize::from_str_radix(
        own.result["hwnd"]
            .as_str()
            .expect("window handle is a string")
            .trim_start_matches("0x"),
        16,
    )
    .expect("window handle is hex");
    let pid = modforge::input::window_pid(console_hwnd).expect("game window has an owning process");
    let game_hwnd = modforge::input::find_hwnd_by_pid(pid).expect("game viewport exists");
    assert!(
        modforge::input::focus_hwnd(game_hwnd),
        "could not focus the game viewport"
    );
    assert_eq!(
        modforge::input::foreground_hwnd(),
        Some(game_hwnd),
        "game viewport did not become foreground"
    );
}

fn input(api: &Api, op: &str, args: serde_json::Value) {
    let response = api.op(op, args);
    assert!(response.ok, "{op} failed: {:?}", response.error);
}

fn route_waypoints(route: &RouteGraph, start: &str, goal: &str) -> Vec<Waypoint> {
    route
        .shortest_path(start, goal, |_| true)
        .unwrap()
        .into_iter()
        .map(|id| route.waypoint(&id).unwrap().clone())
        .collect()
}

fn follow(
    api: &Api,
    selector: &str,
    waypoints: &[Waypoint],
    mut trail: Option<&mut TrailRecorder>,
) -> Result<(), String> {
    let _release = ReleaseForward(api);
    let mut forward_held = false;
    let config = SteeringConfig {
        mouse_units_per_degree: 2.0,
        max_mouse_delta: 120,
        move_yaw_tolerance_deg: 12.0,
    };
    for waypoint in waypoints {
        let started = Instant::now();
        let mut stuck = StuckDetector::new(10.0, 2_000).unwrap();
        let mut forward_calls = 0;
        loop {
            let current = pose(api, selector);
            if let Some(recorder) = trail.as_deref_mut() {
                recorder.observe(current.position);
            }
            let output = steer(current, waypoint, config);
            if output.arrived {
                break;
            }
            if started.elapsed() >= WAYPOINT_TIMEOUT {
                return Err(format!(
                    "timed out at waypoint '{}' from pose {:?}, distance {:.1} cm, yaw error {:.1} deg, forward {}, movement calls {}",
                    waypoint.id,
                    current,
                    output.distance,
                    output.yaw_error_deg,
                    output.forward,
                    forward_calls,
                ));
            }
            if output.forward
                && stuck.observe(started.elapsed().as_millis() as u64, output.distance)
            {
                return Err(format!(
                    "stuck at waypoint '{}' from pose {:?}, distance {:.1} cm, yaw error {:.1} deg, forward {}, movement calls {}",
                    waypoint.id,
                    current,
                    output.distance,
                    output.yaw_error_deg,
                    output.forward,
                    forward_calls,
                ));
            }
            if output.mouse_dx != 0 {
                input(
                    api,
                    "input.mouse.move_rel",
                    json!({"dx": output.mouse_dx, "dy": 0, "backend": "l1"}),
                );
            }
            if output.forward != forward_held {
                input(
                    api,
                    if output.forward {
                        "input.key.down"
                    } else {
                        "input.key.up"
                    },
                    json!({"key": "w", "backend": "l1"}),
                );
                forward_held = output.forward;
            }
            if output.forward {
                forward_calls += 1;
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    }
    Ok(())
}

struct RouteExecutor<'a> {
    api: &'a Api,
    selector: &'a str,
}

impl OpExecutor for RouteExecutor<'_> {
    fn execute(&self, operation: &RecordedOp) -> Result<OpResponse<serde_json::Value>, String> {
        match operation.op.as_str() {
            "route.follow" => {
                let route: RouteGraph = serde_json::from_value(
                    operation
                        .args
                        .get("route")
                        .cloned()
                        .ok_or("route.follow is missing route")?,
                )
                .map_err(|error| format!("parse route.follow route: {error}"))?;
                let start = operation
                    .args
                    .get("start")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("route.follow is missing start")?;
                let goal = operation
                    .args
                    .get("goal")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("route.follow is missing goal")?;
                follow(
                    self.api,
                    self.selector,
                    &route_waypoints(&route, start, goal),
                    None,
                )?;
                Ok(OpResponse::ok(
                    "route.follow",
                    json!({"arrived": goal}),
                    json!({}),
                ))
            }
            "route.arrived" => {
                let target: Position = serde_json::from_value(
                    operation
                        .args
                        .get("position")
                        .cloned()
                        .ok_or("route.arrived is missing position")?,
                )
                .map_err(|error| format!("parse route.arrived position: {error}"))?;
                let radius = operation
                    .args
                    .get("radius")
                    .and_then(serde_json::Value::as_f64)
                    .ok_or("route.arrived is missing radius")?;
                let current = pose(self.api, self.selector);
                Ok(OpResponse::ok(
                    "route.arrived",
                    json!({
                        "arrived": current.position.distance(target) <= radius,
                        "position": current.position,
                        "yaw_deg": current.yaw_deg,
                    }),
                    json!({}),
                ))
            }
            _ => self.api.execute(operation),
        }
    }
}

fn follow_op(route: &RouteGraph, start: &str, goal: &str) -> RecordedOp {
    RecordedOp::new(
        "route.follow",
        json!({"route": route, "start": start, "goal": goal}),
    )
}

fn arrived(position: Position) -> Observation {
    Observation::new(
        RecordedOp::new(
            "route.arrived",
            json!({"position": position, "radius": ARRIVAL_CM}),
        ),
        "/result/arrived",
        json!(true),
    )
}

fn record_traversable_route(api: &Api, selector: &str) -> (Pose, Position, RouteGraph) {
    let mut failures = Vec::new();
    for heading_offset in HEADING_OFFSETS_DEG {
        let start = pose(api, selector);
        let radians = (start.yaw_deg + heading_offset).to_radians();
        let target = Position::new(
            start.position.x + radians.cos() * OUTBOUND_CM,
            start.position.y + radians.sin() * OUTBOUND_CM,
            start.position.z,
        );
        let mut recorder = TrailRecorder::new(SAMPLE_CM, ARRIVAL_CM).unwrap();
        recorder.observe(start.position);
        match follow(
            api,
            selector,
            &[Waypoint::new("recording-target", target, ARRIVAL_CM)],
            Some(&mut recorder),
        ) {
            Ok(()) => {
                let route = recorder.finish("misery short route").unwrap();
                return (start, target, route);
            }
            Err(error) => failures.push(format!("heading {heading_offset:+.0}: {error}")),
        }
    }
    panic!(
        "no local heading produced a traversable route:\n{}",
        failures.join("\n")
    );
}

fn restore_yaw(api: &Api, selector: &str, target_yaw: f64) {
    let config = SteeringConfig {
        mouse_units_per_degree: 2.0,
        max_mouse_delta: 120,
        move_yaw_tolerance_deg: 2.0,
    };
    let started = Instant::now();
    loop {
        let current = pose(api, selector);
        let dx = steer_yaw(current.yaw_deg, target_yaw, config);
        if dx.abs() <= 1 {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "could not restore facing"
        );
        input(
            api,
            "input.mouse.move_rel",
            json!({"dx": dx, "dy": 0, "backend": "l1"}),
        );
        std::thread::sleep(Duration::from_millis(16));
    }
}

#[test]
#[ignore = "moves and restores the live MISERY player"]
fn misery_records_saves_and_replays_a_waypoint_route() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");
    focus_game(&api);

    let player = client::walk_class_chain_instances(&api, PLAYER_CHAIN, 4)
        .into_iter()
        .next()
        .expect("load a save so the live player exists");
    let (start, _target, recorded) = record_traversable_route(&api, &player.addr_selector);
    assert!(
        recorded.waypoints().len() >= 2,
        "route did not retain movement samples"
    );

    let route_path = std::env::temp_dir().join(format!(
        "modforge-misery-waypoint-route-{}.json",
        std::process::id()
    ));
    let _remove_route = RemoveFile(route_path.clone());
    std::fs::write(&route_path, recorded.to_json().unwrap()).unwrap();
    let loaded = RouteGraph::from_json(&std::fs::read_to_string(&route_path).unwrap()).unwrap();
    assert_eq!(loaded, recorded);

    let first = loaded.first_id().unwrap();
    let last = loaded.last_id().unwrap();
    let recorded_goal = loaded.waypoint(last).unwrap().position;
    let reverse = loaded.reversed("misery short route return");
    follow(
        &api,
        &player.addr_selector,
        &route_waypoints(&reverse, last, first),
        None,
    )
    .unwrap();

    let executor = RouteExecutor {
        api: &api,
        selector: &player.addr_selector,
    };
    let mut journal_recorder = Recorder::new("misery short route", &executor);
    journal_recorder
        .action("follow saved route", follow_op(&loaded, first, last))
        .unwrap();
    journal_recorder
        .wait("saved route arrived", arrived(recorded_goal), 250, 25)
        .unwrap();
    journal_recorder
        .action("return on saved route", follow_op(&reverse, last, first))
        .unwrap();
    journal_recorder
        .wait("return arrived", arrived(start.position), 250, 25)
        .unwrap();
    let journal = journal_recorder.finish();

    let journal_path = std::env::temp_dir().join(format!(
        "modforge-misery-waypoint-journal-{}.json",
        std::process::id()
    ));
    let _remove_journal = RemoveFile(journal_path.clone());
    std::fs::write(&journal_path, journal.to_json().unwrap()).unwrap();
    let replay = LiveJournal::from_json(&std::fs::read_to_string(&journal_path).unwrap()).unwrap();
    assert_eq!(replay, journal);
    let report = replay.replay(&executor).unwrap();
    assert_eq!(report.actions, 2);
    assert!(report.wait_polls >= 2);
    restore_yaw(&api, &player.addr_selector, start.yaw_deg);

    let final_pose = pose(&api, &player.addr_selector);
    assert!(
        final_pose.position.distance(start.position) <= ARRIVAL_CM,
        "route did not return to start: start {:?}, final {:?}",
        start,
        final_pose
    );
}
