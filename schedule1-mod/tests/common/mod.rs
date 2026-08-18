//! Shared helpers for the schedule1-mod research tests.
//!
//! The control plane is generic (handle chaining): these helpers
//! are the whole research toolkit. New research tests reuse them;
//! do not copy them into a test file.
//!
//! Each --test target compiles this module separately and no
//! target uses every helper, so dead_code is expected here.
#![allow(dead_code, unused_imports)]

pub use unityforge::client::{
    count_of, dump_sequence, field_exists, fields, find_instances,
    first_handle, handle_of, parse_vec3, ping_or_skip,
    print_declared_methods,
};

use unityforge::client::Api;
use serde_json::{Value, json};

pub fn api() -> Api<Value> {
    let port = std::env::var("SCHEDULE1_MOD_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(17175);
    Api::at(port, "/op")
}

/// Convenience wrapper: find_instances returning Option (for
/// callers that don't care about the error).
pub fn walk(api: &Api<Value>, class: &str) -> Option<Vec<Value>> {
    find_instances(api, class, false).ok()
}

/// The local player's world position via Player.transform.
pub fn player_position(api: &Api<Value>) -> Option<(f64, f64, f64)> {
    let player = first_handle(api, "ScheduleOne.PlayerScripts.Player")?;
    let transform = api.op("read_field", json!({"handle": player, "field": "transform"}));
    let th = handle_of(&transform.result)?;
    let pos = api.op("invoke_method", json!({"handle": th, "method": "get_position", "args": []}));
    parse_vec3(&pos.result)
}
