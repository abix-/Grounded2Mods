use modforge::route::{
    Pose, Position, RouteEdge, RouteGraph, SteeringConfig, StuckDetector, TrailRecorder, Waypoint,
    steer, steer_yaw,
};

fn position(x: f64, y: f64, z: f64) -> Position {
    Position::new(x, y, z)
}

#[test]
fn recorded_trail_reduces_samples_and_round_trips() {
    let mut recorder = TrailRecorder::new(50.0, 20.0).unwrap();
    assert!(recorder.observe(position(0.0, 0.0, 0.0)));
    assert!(!recorder.observe(position(10.0, 0.0, 0.0)));
    assert!(recorder.observe(position(60.0, 0.0, 0.0)));
    assert!(!recorder.observe(position(75.0, 0.0, 0.0)));

    let route = recorder.finish("spawn to expedition").unwrap();
    assert_eq!(route.waypoints().len(), 3, "the final observed position is retained");
    assert_eq!(route.edges().len(), 2);
    assert_eq!(route.first_id(), Some("wp-0000"));
    assert_eq!(route.last_id(), Some("wp-0002"));

    let loaded = RouteGraph::from_json(&route.to_json().unwrap()).unwrap();
    assert_eq!(loaded, route);
}

#[test]
fn astar_uses_an_alternate_recorded_edge_when_the_direct_edge_is_blocked() {
    let graph = RouteGraph::new(
        "fork",
        vec![
            Waypoint::new("start", position(0.0, 0.0, 0.0), 10.0),
            Waypoint::new("fork", position(10.0, 10.0, 0.0), 10.0),
            Waypoint::new("goal", position(20.0, 0.0, 0.0), 10.0),
        ],
        vec![
            RouteEdge::new("direct", "start", "goal", 20.0),
            RouteEdge::new("around-a", "start", "fork", 15.0),
            RouteEdge::new("around-b", "fork", "goal", 15.0),
        ],
    )
    .unwrap();

    assert_eq!(
        graph.shortest_path("start", "goal", |edge| edge.id != "direct").unwrap(),
        vec!["start", "fork", "goal"]
    );
}

#[test]
fn steering_takes_the_short_turn_across_the_yaw_wrap() {
    let wanted = (-179.0f64).to_radians();
    let waypoint = Waypoint::new(
        "turn",
        position(wanted.cos() * 100.0, wanted.sin() * 100.0, 0.0),
        10.0,
    );
    let config = SteeringConfig {
        mouse_units_per_degree: 2.0,
        max_mouse_delta: 100,
        move_yaw_tolerance_deg: 10.0,
    };
    let output = steer(
        Pose::new(position(0.0, 0.0, 0.0), 179.0),
        &waypoint,
        config,
    );

    assert!(!output.arrived);
    assert!(output.forward, "a two-degree correction may keep moving");
    assert!((output.yaw_error_deg - 2.0).abs() < 0.01);
    assert_eq!(output.mouse_dx, 4);
}

#[test]
fn steering_stops_inside_the_waypoint_radius_and_can_restore_facing() {
    let waypoint = Waypoint::new("near", position(5.0, 0.0, 3.0), 10.0);
    let config = SteeringConfig::default();
    let output = steer(
        Pose::new(position(0.0, 0.0, 0.0), 90.0),
        &waypoint,
        config,
    );

    assert!(output.arrived);
    assert!(!output.forward);
    assert_eq!(output.mouse_dx, 0);
    assert_eq!(steer_yaw(-179.0, 179.0, config), -2);
}

#[test]
fn stuck_detection_resets_only_after_measured_progress() {
    let mut stuck = StuckDetector::new(10.0, 1_000).unwrap();
    assert!(!stuck.observe(0, 100.0));
    assert!(!stuck.observe(900, 95.0));
    assert!(stuck.observe(1_000, 95.0));

    assert!(!stuck.observe(1_100, 70.0));
    assert!(!stuck.observe(2_000, 65.0));
    assert!(stuck.observe(2_100, 65.0));
}
