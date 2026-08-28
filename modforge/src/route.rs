//! Shared bot navigation for Unreal and Unity games.

use serde::{Deserialize, Serialize};
use smallvec::{SmallVec, smallvec};

use crate::input::{Key, PlayerCommand};

const W: Key = Key(0x57);
const A: Key = Key(0x41);
const S: Key = Key(0x53);
const D: Key = Key(0x44);

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Position {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn distance(self, other: Self) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        let dz = other.z - self.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Waypoint {
    pub id: String,
    pub position: Position,
    pub arrival_radius: f64,
}

impl Waypoint {
    pub fn new(id: impl Into<String>, position: Position, arrival_radius: f64) -> Self {
        Self {
            id: id.into(),
            position,
            arrival_radius,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub name: String,
    waypoints: Vec<Waypoint>,
}

impl Route {
    pub fn new(name: impl Into<String>, waypoints: Vec<Waypoint>) -> Result<Self, String> {
        let route = Self {
            name: name.into(),
            waypoints,
        };
        route.validate()?;
        Ok(route)
    }

    pub fn waypoints(&self) -> &[Waypoint] {
        &self.waypoints
    }

    pub fn waypoint(&self, id: &str) -> Option<&Waypoint> {
        self.waypoints.iter().find(|waypoint| waypoint.id == id)
    }

    pub fn waypoints_after(&self, start: &str, goal: &str) -> Result<Vec<Waypoint>, String> {
        let start_index = self.index(start)?;
        let goal_index = self.index(goal)?;
        if start_index < goal_index {
            Ok(self.waypoints[start_index + 1..=goal_index].to_vec())
        } else if start_index > goal_index {
            Ok(self.waypoints[goal_index..start_index]
                .iter()
                .rev()
                .cloned()
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    fn index(&self, id: &str) -> Result<usize, String> {
        self.waypoints
            .iter()
            .position(|waypoint| waypoint.id == id)
            .ok_or_else(|| format!("route waypoint '{id}' does not exist"))
    }

    fn validate(&self) -> Result<(), String> {
        if self.waypoints.is_empty() {
            return Err("route has no waypoints".into());
        }
        for (index, waypoint) in self.waypoints.iter().enumerate() {
            if waypoint.id.is_empty() {
                return Err(format!("waypoint {index} has an empty id"));
            }
            if self.waypoints[..index]
                .iter()
                .any(|earlier| earlier.id == waypoint.id)
            {
                return Err(format!("duplicate waypoint id '{}'", waypoint.id));
            }
            if !waypoint.position.is_finite() {
                return Err(format!(
                    "waypoint '{}' has an invalid position",
                    waypoint.id
                ));
            }
            if !waypoint.arrival_radius.is_finite() || waypoint.arrival_radius <= 0.0 {
                return Err(format!(
                    "waypoint '{}' arrival radius must be finite and positive",
                    waypoint.id
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathPoint {
    pub position: Position,
}

impl PathPoint {
    pub const fn new(position: Position) -> Self {
        Self { position }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Path {
    points: Vec<PathPoint>,
}

impl Path {
    pub fn new(points: Vec<PathPoint>) -> Result<Self, String> {
        if points.is_empty() {
            return Err("path has no path points".into());
        }
        if points.iter().any(|point| !point.position.is_finite()) {
            return Err("path contains an invalid path point".into());
        }
        Ok(Self { points })
    }

    pub fn points(&self) -> &[PathPoint] {
        &self.points
    }

    pub fn cost(&self) -> f64 {
        self.points
            .windows(2)
            .map(|pair| pair[0].position.distance(pair[1].position))
            .sum()
    }
}

pub trait GameNavigation: Send + Sync {
    fn find_path(&self, start: Position, goal: Position) -> Result<Path, String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerObservation {
    pub position: Position,
    pub yaw_deg: f64,
    pub pitch_deg: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SteeringConfig {
    pub mouse_units_per_degree: f64,
    pub max_mouse_delta: i32,
    pub move_yaw_tolerance_deg: f64,
    pub path_point_radius: f64,
}

impl Default for SteeringConfig {
    fn default() -> Self {
        Self {
            mouse_units_per_degree: 1.0,
            max_mouse_delta: 120,
            move_yaw_tolerance_deg: 10.0,
            path_point_radius: 75.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steering {
    pub arrived: bool,
    pub forward: bool,
    pub mouse_dx: i32,
    pub yaw_error_deg: f64,
    pub distance: f64,
}

pub fn steer(
    player: PlayerObservation,
    target: Position,
    arrival_radius: f64,
    config: SteeringConfig,
) -> Steering {
    let distance = player.position.distance(target);
    if distance <= arrival_radius {
        return Steering {
            arrived: true,
            forward: false,
            mouse_dx: 0,
            yaw_error_deg: 0.0,
            distance,
        };
    }

    let desired_yaw = (target.y - player.position.y)
        .atan2(target.x - player.position.x)
        .to_degrees();
    let yaw_error_deg = yaw_error(player.yaw_deg, desired_yaw);
    Steering {
        arrived: false,
        forward: yaw_error_deg.abs() <= config.move_yaw_tolerance_deg,
        mouse_dx: mouse_delta(yaw_error_deg, config),
        yaw_error_deg,
        distance,
    }
}

pub fn steer_yaw(current_yaw_deg: f64, target_yaw_deg: f64, config: SteeringConfig) -> i32 {
    mouse_delta(yaw_error(current_yaw_deg, target_yaw_deg), config)
}

fn yaw_error(current_yaw_deg: f64, target_yaw_deg: f64) -> f64 {
    let mut error = (target_yaw_deg - current_yaw_deg) % 360.0;
    if error > 180.0 {
        error -= 360.0;
    } else if error < -180.0 {
        error += 360.0;
    }
    error
}

fn mouse_delta(yaw_error_deg: f64, config: SteeringConfig) -> i32 {
    if !yaw_error_deg.is_finite() || !config.mouse_units_per_degree.is_finite() {
        return 0;
    }
    (yaw_error_deg * config.mouse_units_per_degree)
        .round()
        .clamp(
            -(config.max_mouse_delta.max(0) as f64),
            config.max_mouse_delta.max(0) as f64,
        ) as i32
}

pub struct StuckDetector {
    min_progress: f64,
    stuck_after_ms: u64,
    best_distance: Option<f64>,
    last_progress_ms: u64,
}

impl StuckDetector {
    pub fn new(min_progress: f64, stuck_after_ms: u64) -> Result<Self, String> {
        if !min_progress.is_finite() || min_progress <= 0.0 {
            return Err("minimum progress must be finite and positive".into());
        }
        if stuck_after_ms == 0 {
            return Err("stuck interval must be positive".into());
        }
        Ok(Self {
            min_progress,
            stuck_after_ms,
            best_distance: None,
            last_progress_ms: 0,
        })
    }

    pub fn observe(&mut self, now_ms: u64, distance: f64) -> bool {
        let Some(best) = self.best_distance else {
            self.best_distance = Some(distance);
            self.last_progress_ms = now_ms;
            return false;
        };
        if best - distance >= self.min_progress {
            self.best_distance = Some(distance);
            self.last_progress_ms = now_ms;
            return false;
        }
        now_ms.saturating_sub(self.last_progress_ms) >= self.stuck_after_ms
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BotStatus {
    Travelling { path_index: usize },
    Arrived,
    Stuck,
    Cancelled,
}

pub type PlayerCommands = SmallVec<[PlayerCommand; 5]>;

#[derive(Clone, Debug, PartialEq)]
pub struct BotOutput {
    pub status: BotStatus,
    pub commands: PlayerCommands,
}

pub struct Bot {
    path: Path,
    path_index: usize,
    goal_arrival_radius: f64,
    steering: SteeringConfig,
    min_progress: f64,
    stuck_after_ms: u64,
    stuck: StuckDetector,
    terminal: Option<BotStatus>,
}

impl Bot {
    pub fn new(
        path: Path,
        goal_arrival_radius: f64,
        steering: SteeringConfig,
        min_progress: f64,
        stuck_after_ms: u64,
    ) -> Result<Self, String> {
        if !goal_arrival_radius.is_finite() || goal_arrival_radius <= 0.0 {
            return Err("goal arrival radius must be finite and positive".into());
        }
        if !steering.path_point_radius.is_finite() || steering.path_point_radius <= 0.0 {
            return Err("path-point radius must be finite and positive".into());
        }
        let stuck = StuckDetector::new(min_progress, stuck_after_ms)?;
        Ok(Self {
            path,
            path_index: 0,
            goal_arrival_radius,
            steering,
            min_progress,
            stuck_after_ms,
            stuck,
            terminal: None,
        })
    }

    pub fn tick(&mut self, player: PlayerObservation, now_ms: u64) -> BotOutput {
        if let Some(status) = self.terminal {
            return BotOutput {
                status,
                commands: PlayerCommands::new(),
            };
        }

        loop {
            let final_point = self.path_index + 1 == self.path.points.len();
            let arrival_radius = if final_point {
                self.goal_arrival_radius
            } else {
                self.steering.path_point_radius
            };
            let steering = steer(
                player,
                self.path.points[self.path_index].position,
                arrival_radius,
                self.steering,
            );
            if !steering.arrived {
                if self.stuck.observe(now_ms, steering.distance) {
                    self.terminal = Some(BotStatus::Stuck);
                    return BotOutput {
                        status: BotStatus::Stuck,
                        commands: release_movement(),
                    };
                }
                return BotOutput {
                    status: BotStatus::Travelling {
                        path_index: self.path_index,
                    },
                    commands: travel_commands(steering.mouse_dx, steering.forward),
                };
            }

            self.path_index += 1;
            if self.path_index == self.path.points.len() {
                self.terminal = Some(BotStatus::Arrived);
                return BotOutput {
                    status: BotStatus::Arrived,
                    commands: release_movement(),
                };
            }
            self.stuck = StuckDetector::new(self.min_progress, self.stuck_after_ms)
                .expect("bot progress settings were validated at construction");
        }
    }

    pub fn cancel(&mut self) -> BotOutput {
        self.terminal = Some(BotStatus::Cancelled);
        BotOutput {
            status: BotStatus::Cancelled,
            commands: release_movement(),
        }
    }
}

fn travel_commands(mouse_dx: i32, forward: bool) -> PlayerCommands {
    let mut commands = PlayerCommands::new();
    if mouse_dx != 0 {
        commands.push(PlayerCommand::mouse_delta(mouse_dx, 0));
    }
    commands.extend([
        PlayerCommand::key(W, forward),
        PlayerCommand::key(A, false),
        PlayerCommand::key(S, false),
        PlayerCommand::key(D, false),
    ]);
    commands
}

fn release_movement() -> PlayerCommands {
    smallvec![
        PlayerCommand::key(W, false),
        PlayerCommand::key(A, false),
        PlayerCommand::key(S, false),
        PlayerCommand::key(D, false),
    ]
}
