//! Engine-independent upgrade levels, policy math, and persistence.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde_json::{Map, Value as Json, json};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpgradeStatus {
    pub entities_upgraded: usize,
    pub levels_total: i64,
    pub levels_per_track: BTreeMap<String, i64>,
}

#[derive(Default)]
struct StoreState {
    slot: Option<i64>,
    path: Option<PathBuf>,
    scopes: Map<String, Json>,
}

/// Upgrade levels keyed by a caller-defined scope, stable entity id,
/// and stable track name.
pub struct UpgradeStore {
    schema_version: i64,
    state: Mutex<Option<StoreState>>,
}

impl UpgradeStore {
    pub const fn new(schema_version: i64) -> Self {
        Self {
            schema_version,
            state: Mutex::new(None),
        }
    }

    pub fn load(&self, slot: i64, path: PathBuf) -> Result<(), String> {
        let mut state = self.state.lock();
        if state
            .as_ref()
            .is_some_and(|state| state.slot == Some(slot) && state.path.as_ref() == Some(&path))
        {
            return Ok(());
        }

        *state = Some(StoreState {
            slot: Some(slot),
            path: Some(path.clone()),
            scopes: Map::new(),
        });
        let mut scopes = Map::new();
        if path.exists() {
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("read {}: {error}", path.display()))?;
            let root: Json = serde_json::from_str(&text)
                .map_err(|error| format!("parse {}: {error}", path.display()))?;
            let object = root
                .as_object()
                .ok_or_else(|| format!("parse {}: root is not an object", path.display()))?;
            for (name, value) in object {
                if name != "schema_version" {
                    scopes.insert(name.clone(), value.clone());
                }
            }
        }

        state
            .as_mut()
            .expect("upgrade state was initialized")
            .scopes = scopes;
        Ok(())
    }

    pub fn ensure_scope(&self, scope: &str) -> Result<(), String> {
        let mut state = self.state.lock();
        let state = state
            .as_mut()
            .ok_or_else(|| "upgrade store is not loaded".to_string())?;
        state
            .scopes
            .entry(scope.to_string())
            .or_insert_with(|| json!({}));
        Ok(())
    }

    pub fn level(&self, scope: &str, entity_id: i64, track: &str) -> i64 {
        self.state
            .lock()
            .as_ref()
            .and_then(|state| state.scopes.get(scope))
            .and_then(Json::as_object)
            .and_then(|entities| entities.get(&entity_id.to_string()))
            .and_then(Json::as_object)
            .and_then(|tracks| tracks.get(track))
            .and_then(Json::as_i64)
            .unwrap_or(0)
    }

    pub fn set_level(
        &self,
        scope: &str,
        entity_id: i64,
        track: &str,
        level: i64,
    ) -> Result<(), String> {
        {
            let mut state = self.state.lock();
            let state = state
                .as_mut()
                .ok_or_else(|| "upgrade store is not loaded".to_string())?;
            let entities = state
                .scopes
                .entry(scope.to_string())
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| format!("upgrade scope '{scope}' is not an object"))?;
            let tracks = entities
                .entry(entity_id.to_string())
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| format!("upgrade entity '{entity_id}' is not an object"))?;
            tracks.insert(track.to_string(), json!(level));
        }
        self.persist()
    }

    pub fn has_any(&self, scope: &str, track: &str) -> bool {
        self.state
            .lock()
            .as_ref()
            .and_then(|state| state.scopes.get(scope))
            .and_then(Json::as_object)
            .is_some_and(|entities| {
                entities.values().any(|entity| {
                    entity
                        .as_object()
                        .and_then(|tracks| tracks.get(track))
                        .and_then(Json::as_i64)
                        .is_some_and(|level| level > 0)
                })
            })
    }

    pub fn status(&self, scope: &str) -> UpgradeStatus {
        let state = self.state.lock();
        let Some(entities) = state
            .as_ref()
            .and_then(|state| state.scopes.get(scope))
            .and_then(Json::as_object)
        else {
            return UpgradeStatus::default();
        };
        let mut status = UpgradeStatus {
            entities_upgraded: entities.len(),
            ..UpgradeStatus::default()
        };
        for entity in entities.values().filter_map(Json::as_object) {
            for (track, level) in entity {
                let level = level.as_i64().unwrap_or(0);
                *status.levels_per_track.entry(track.clone()).or_default() += level;
                status.levels_total += level;
            }
        }
        status
    }

    pub fn slot(&self) -> Option<i64> {
        self.state.lock().as_ref().and_then(|state| state.slot)
    }

    fn persist(&self) -> Result<(), String> {
        let state = self.state.lock();
        let state = state
            .as_ref()
            .ok_or_else(|| "upgrade store is not loaded".to_string())?;
        let Some(path) = state.path.as_deref() else {
            return Err("upgrade store is not loaded".to_string());
        };
        let mut root = state.scopes.clone();
        root.insert("schema_version".to_string(), json!(self.schema_version));
        let text = Json::Object(root).to_string();
        let tmp = temporary_path(path);
        std::fs::write(&tmp, text).map_err(|error| format!("write {}: {error}", tmp.display()))?;
        if path.exists() {
            std::fs::remove_file(path)
                .map_err(|error| format!("replace {}: {error}", path.display()))?;
        }
        std::fs::rename(&tmp, path).map_err(|error| format!("replace {}: {error}", path.display()))
    }
}

pub fn cost(base_need: f64, factor: i64, next_level: i64) -> i64 {
    (base_need.ceil() as i64).max(1) * factor * next_level
}

pub fn skill_requirement(base: i64, levels_per_band: i64, next_level: i64) -> i64 {
    base + (next_level - 1) / levels_per_band
}

pub fn diminishing_bonus(level: i64, base_step: f32, decay: f32) -> f32 {
    let mut bonus = 0.0;
    let mut step = base_step;
    for _ in 0..level.max(0) {
        bonus += step;
        step *= decay;
    }
    bonus
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(".tmp");
    PathBuf::from(temporary)
}
