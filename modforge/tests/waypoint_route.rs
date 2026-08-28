use modforge::input::{Key, PlayerCommand};
use modforge::route::{
    Bot, BotStatus, Path, PathPoint, PlayerObservation, Position, Route, SteeringConfig,
    StuckDetector, Waypoint,
};

fn position(x: f64, y: f64, z: f64) -> Position {
    Position::new(x, y, z)
}

fn observation(x: f64, y: f64, yaw_deg: f64) -> PlayerObservation {
    PlayerObservation {
        position: position(x, y, 0.0),
        yaw_deg,
        pitch_deg: 0.0,
    }
}

#[test]
fn route_is_an_ordered_list_of_stops_without_pathfinding() {
    let route = Route::new(
        "spawn to expedition",
        vec![
            Waypoint::new("spawn", position(0.0, 0.0, 0.0), 20.0),
            Waypoint::new("metal-door", position(100.0, 0.0, 0.0), 20.0),
            Waypoint::new("expedition-door", position(200.0, 0.0, 0.0), 20.0),
        ],
    )
    .unwrap();

    assert_eq!(
        route
            .waypoints_after("spawn", "expedition-door")
            .unwrap()
            .iter()
            .map(|waypoint| waypoint.id.as_str())
            .collect::<Vec<_>>(),
        vec!["metal-door", "expedition-door"]
    );
    assert!(route.waypoints_after("expedition-door", "spawn").is_err());
}

#[test]
fn path_points_are_separate_from_the_goal_waypoint() {
    let path = Path::new(vec![
        PathPoint::new(position(50.0, 0.0, 0.0)),
        PathPoint::new(position(100.0, 0.0, 0.0)),
    ])
    .unwrap();
    assert_eq!(path.points().len(), 2);
    assert_eq!(path.cost(), 50.0);
}

#[test]
fn bot_reads_path_and_returns_only_player_input() {
    let path = Path::new(vec![
        PathPoint::new(position(100.0, 0.0, 0.0)),
        PathPoint::new(position(100.0, 100.0, 0.0)),
    ])
    .unwrap();
    let mut bot = Bot::new(path, 5.0, SteeringConfig::default(), 1.0, 1_000).unwrap();

    let first = bot.tick(observation(0.0, 0.0, 0.0), 0);
    assert_eq!(first.status, BotStatus::Travelling { path_index: 0 });
    assert!(
        first
            .commands
            .contains(&PlayerCommand::key(Key(0x57), true))
    );

    let turn = bot.tick(observation(100.0, 0.0, 0.0), 100);
    assert_eq!(turn.status, BotStatus::Travelling { path_index: 1 });
    assert!(
        turn.commands
            .iter()
            .any(|command| matches!(command, PlayerCommand::MouseDelta { dx, dy: 0 } if *dx > 0))
    );
    assert!(
        turn.commands
            .contains(&PlayerCommand::key(Key(0x57), false))
    );

    let arrived = bot.tick(observation(100.0, 100.0, 90.0), 200);
    assert_eq!(arrived.status, BotStatus::Arrived);
    assert_all_movement_released(&arrived.commands);
}

#[test]
fn identical_unreal_and_unity_inputs_receive_identical_commands() {
    let path = Path::new(vec![PathPoint::new(position(100.0, 0.0, 0.0))]).unwrap();
    let mut unreal_bot =
        Bot::new(path.clone(), 5.0, SteeringConfig::default(), 1.0, 1_000).unwrap();
    let mut unity_bot = Bot::new(path, 5.0, SteeringConfig::default(), 1.0, 1_000).unwrap();
    let player = observation(0.0, 0.0, 0.0);

    assert_eq!(
        unreal_bot.tick(player, 0).commands,
        unity_bot.tick(player, 0).commands
    );
}

#[test]
fn bot_releases_every_movement_key_when_stuck_or_cancelled() {
    let path = Path::new(vec![PathPoint::new(position(100.0, 0.0, 0.0))]).unwrap();
    let mut stuck_bot = Bot::new(path.clone(), 5.0, SteeringConfig::default(), 1.0, 100).unwrap();
    let player = observation(0.0, 0.0, 0.0);

    stuck_bot.tick(player, 0);
    let stuck = stuck_bot.tick(player, 100);
    assert_eq!(stuck.status, BotStatus::Stuck);
    assert_all_movement_released(&stuck.commands);

    let mut cancelled_bot = Bot::new(path, 5.0, SteeringConfig::default(), 1.0, 100).unwrap();
    let cancelled = cancelled_bot.cancel();
    assert_eq!(cancelled.status, BotStatus::Cancelled);
    assert_all_movement_released(&cancelled.commands);
}

fn assert_all_movement_released(commands: &[PlayerCommand]) {
    for key in [0x57, 0x41, 0x53, 0x44] {
        assert!(commands.contains(&PlayerCommand::key(Key(key), false)));
    }
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
