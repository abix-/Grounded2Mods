//! Recorded waypoint routes, A* selection, and closed-loop steering.
//!
//! A route contains only edges that were traversed successfully while
//! recording. The graph selects a route; a host-specific follower observes
//! the live pose and applies [`steer`] until each waypoint is reached.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "modforge.route@v1";

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
pub struct RouteEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub cost: f64,
}

impl RouteEdge {
    pub fn new(
        id: impl Into<String>,
        from: impl Into<String>,
        to: impl Into<String>,
        cost: f64,
    ) -> Self {
        Self {
            id: id.into(),
            from: from.into(),
            to: to.into(),
            cost,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouteGraph {
    schema: String,
    pub name: String,
    waypoints: Vec<Waypoint>,
    edges: Vec<RouteEdge>,
}

impl RouteGraph {
    pub fn new(
        name: impl Into<String>,
        waypoints: Vec<Waypoint>,
        edges: Vec<RouteEdge>,
    ) -> Result<Self, String> {
        let graph = Self {
            schema: SCHEMA.to_string(),
            name: name.into(),
            waypoints,
            edges,
        };
        graph.validate()?;
        Ok(graph)
    }

    pub fn waypoints(&self) -> &[Waypoint] {
        &self.waypoints
    }

    pub fn edges(&self) -> &[RouteEdge] {
        &self.edges
    }

    pub fn waypoint(&self, id: &str) -> Option<&Waypoint> {
        self.waypoints.iter().find(|waypoint| waypoint.id == id)
    }

    pub fn first_id(&self) -> Option<&str> {
        self.waypoints.first().map(|waypoint| waypoint.id.as_str())
    }

    pub fn last_id(&self) -> Option<&str> {
        self.waypoints.last().map(|waypoint| waypoint.id.as_str())
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|error| format!("serialize route: {error}"))
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let graph: Self =
            serde_json::from_str(text).map_err(|error| format!("parse route: {error}"))?;
        if graph.schema != SCHEMA {
            return Err(format!(
                "unsupported route schema '{}' (expected '{SCHEMA}')",
                graph.schema
            ));
        }
        graph.validate()?;
        Ok(graph)
    }

    pub fn reversed(&self, name: impl Into<String>) -> Self {
        let edges = self
            .edges
            .iter()
            .map(|edge| {
                RouteEdge::new(
                    format!("reverse:{}", edge.id),
                    edge.to.clone(),
                    edge.from.clone(),
                    edge.cost,
                )
            })
            .collect();
        Self::new(name, self.waypoints.clone(), edges)
            .expect("reversing a valid graph preserves graph validity")
    }

    pub fn shortest_path(
        &self,
        start: &str,
        goal: &str,
        edge_is_available: impl Fn(&RouteEdge) -> bool,
    ) -> Result<Vec<String>, String> {
        let indices: HashMap<&str, usize> = self
            .waypoints
            .iter()
            .enumerate()
            .map(|(index, waypoint)| (waypoint.id.as_str(), index))
            .collect();
        let start_index = *indices
            .get(start)
            .ok_or_else(|| format!("route start waypoint '{start}' does not exist"))?;
        let goal_index = *indices
            .get(goal)
            .ok_or_else(|| format!("route goal waypoint '{goal}' does not exist"))?;

        let mut best = vec![f64::INFINITY; self.waypoints.len()];
        let mut came_from = vec![None; self.waypoints.len()];
        let mut open = BinaryHeap::new();
        best[start_index] = 0.0;
        open.push(OpenNode {
            estimate: self.waypoints[start_index]
                .position
                .distance(self.waypoints[goal_index].position),
            cost: 0.0,
            index: start_index,
        });

        while let Some(current) = open.pop() {
            if current.cost > best[current.index] {
                continue;
            }
            if current.index == goal_index {
                let mut path = vec![goal_index];
                let mut at = goal_index;
                while let Some(previous) = came_from[at] {
                    path.push(previous);
                    at = previous;
                }
                path.reverse();
                return Ok(path
                    .into_iter()
                    .map(|index| self.waypoints[index].id.clone())
                    .collect());
            }

            let current_id = self.waypoints[current.index].id.as_str();
            for edge in self
                .edges
                .iter()
                .filter(|edge| edge.from == current_id && edge_is_available(edge))
            {
                let next = indices[edge.to.as_str()];
                let next_cost = current.cost + edge.cost;
                if next_cost >= best[next] {
                    continue;
                }
                best[next] = next_cost;
                came_from[next] = Some(current.index);
                let heuristic = self.waypoints[next]
                    .position
                    .distance(self.waypoints[goal_index].position);
                open.push(OpenNode {
                    estimate: next_cost + heuristic,
                    cost: next_cost,
                    index: next,
                });
            }
        }

        Err(format!(
            "no available recorded route from '{start}' to '{goal}'"
        ))
    }

    fn validate(&self) -> Result<(), String> {
        if self.waypoints.is_empty() {
            return Err("route has no waypoints".into());
        }
        let mut indices = HashMap::with_capacity(self.waypoints.len());
        for (index, waypoint) in self.waypoints.iter().enumerate() {
            if waypoint.id.is_empty() {
                return Err(format!("waypoint {index} has an empty id"));
            }
            if indices.insert(waypoint.id.as_str(), index).is_some() {
                return Err(format!("duplicate waypoint id '{}'", waypoint.id));
            }
            if !waypoint.position.is_finite() {
                return Err(format!(
                    "waypoint '{}' has a non-finite position",
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

        let mut edge_ids = HashMap::with_capacity(self.edges.len());
        for edge in &self.edges {
            if edge.id.is_empty() {
                return Err("route edge has an empty id".into());
            }
            if edge_ids.insert(edge.id.as_str(), ()).is_some() {
                return Err(format!("duplicate route edge id '{}'", edge.id));
            }
            let Some(&from) = indices.get(edge.from.as_str()) else {
                return Err(format!(
                    "route edge '{}' starts at missing waypoint '{}'",
                    edge.id, edge.from
                ));
            };
            let Some(&to) = indices.get(edge.to.as_str()) else {
                return Err(format!(
                    "route edge '{}' ends at missing waypoint '{}'",
                    edge.id, edge.to
                ));
            };
            let straight = self.waypoints[from]
                .position
                .distance(self.waypoints[to].position);
            if !edge.cost.is_finite() || edge.cost + f64::EPSILON < straight {
                return Err(format!(
                    "route edge '{}' cost must be finite and at least its {:.3} straight-line distance",
                    edge.id, straight
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
struct OpenNode {
    estimate: f64,
    cost: f64,
    index: usize,
}

impl PartialEq for OpenNode {
    fn eq(&self, other: &Self) -> bool {
        self.estimate == other.estimate && self.index == other.index
    }
}

impl Eq for OpenNode {}

impl PartialOrd for OpenNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OpenNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .total_cmp(&self.estimate)
            .then_with(|| other.index.cmp(&self.index))
    }
}

pub struct TrailRecorder {
    sample_spacing: f64,
    arrival_radius: f64,
    retained: Vec<Position>,
    last_observed: Option<Position>,
}

impl TrailRecorder {
    pub fn new(sample_spacing: f64, arrival_radius: f64) -> Result<Self, String> {
        if !sample_spacing.is_finite() || sample_spacing <= 0.0 {
            return Err("trail sample spacing must be finite and positive".into());
        }
        if !arrival_radius.is_finite() || arrival_radius <= 0.0 {
            return Err("trail arrival radius must be finite and positive".into());
        }
        Ok(Self {
            sample_spacing,
            arrival_radius,
            retained: Vec::new(),
            last_observed: None,
        })
    }

    pub fn observe(&mut self, position: Position) -> bool {
        if !position.is_finite() {
            return false;
        }
        self.last_observed = Some(position);
        let retain = self
            .retained
            .last()
            .is_none_or(|last| last.distance(position) >= self.sample_spacing);
        if retain {
            self.retained.push(position);
        }
        retain
    }

    pub fn finish(mut self, name: impl Into<String>) -> Result<RouteGraph, String> {
        if let Some(last) = self.last_observed
            && self
                .retained
                .last()
                .is_none_or(|retained| retained.distance(last) > f64::EPSILON)
        {
            self.retained.push(last);
        }
        if self.retained.len() < 2 {
            return Err("recorded trail requires at least two distinct positions".into());
        }

        let waypoints: Vec<_> = self
            .retained
            .iter()
            .enumerate()
            .map(|(index, position)| {
                Waypoint::new(format!("wp-{index:04}"), *position, self.arrival_radius)
            })
            .collect();
        let edges = waypoints
            .windows(2)
            .map(|pair| {
                RouteEdge::new(
                    format!("{}->{}", pair[0].id, pair[1].id),
                    pair[0].id.clone(),
                    pair[1].id.clone(),
                    pair[0].position.distance(pair[1].position),
                )
            })
            .collect();
        RouteGraph::new(name, waypoints, edges)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub position: Position,
    pub yaw_deg: f64,
}

impl Pose {
    pub const fn new(position: Position, yaw_deg: f64) -> Self {
        Self { position, yaw_deg }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SteeringConfig {
    pub mouse_units_per_degree: f64,
    pub max_mouse_delta: i32,
    pub move_yaw_tolerance_deg: f64,
}

impl Default for SteeringConfig {
    fn default() -> Self {
        Self {
            mouse_units_per_degree: 1.0,
            max_mouse_delta: 120,
            move_yaw_tolerance_deg: 10.0,
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

pub fn steer(pose: Pose, waypoint: &Waypoint, config: SteeringConfig) -> Steering {
    let distance = pose.position.distance(waypoint.position);
    if distance <= waypoint.arrival_radius {
        return Steering {
            arrived: true,
            forward: false,
            mouse_dx: 0,
            yaw_error_deg: 0.0,
            distance,
        };
    }

    let dx = waypoint.position.x - pose.position.x;
    let dy = waypoint.position.y - pose.position.y;
    let desired_yaw = dy.atan2(dx).to_degrees();
    let yaw_error_deg = yaw_error(pose.yaw_deg, desired_yaw);
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
