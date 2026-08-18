//! Shared helpers for the wwm-mod research tests.
//!
//! WWM is Unity Mono, so class names are plain (no Il2Cpp
//! prefix) and every op chains on the shim's object handles.
//! New research tests reuse these; do not copy them into a
//! test file.
//!
//! Each --test target compiles this module separately and no
//! target uses every helper, so dead_code is expected here.
#![allow(dead_code, unused_imports)]

pub use unityforge::client::{
    count_of, dump_sequence, fields, find_instances,
    handle_of, parse_vec3, ping_or_skip, print_declared_methods,
};
pub use unityforge::client::first_handle_inactive as first_handle;

use unityforge::client::Api;
use serde_json::{Value, json};

/// Control plane port. Override with WWM_MOD_PORT.
pub fn api() -> Api<Value> {
    let port = std::env::var("WWM_MOD_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(17172);
    Api::at(port, "/op").with_timeout(std::time::Duration::from_secs(30))
}

/// True when inspect_object on a live instance of `class` lists
/// `field`. Uses include_inactive (WWM parks managers on inactive
/// GameObjects).
pub fn field_exists(api: &Api<Value>, class: &str, field: &str) -> bool {
    let Some(handle) = first_handle(api, class) else {
        println!("  {class}.{field}: NO LIVE INSTANCE (class may still exist)");
        return false;
    };
    let read = api.op("read_field", json!({"handle": handle, "field": field}));
    if read.ok {
        println!("  {class}.{field} = {}", read.result);
        true
    } else {
        println!("  {class}.{field}: MISSING ({:?})", read.error);
        false
    }
}

/// True only when the failure text says the type itself is absent.
pub fn is_type_not_found(err: &str) -> bool {
    err.contains("not found")
}

/// find_instances with include_inactive=true, returning Result
/// (WWM parks managers on inactive GameObjects).
pub fn try_walk(api: &Api<Value>, class: &str) -> Result<Vec<Value>, String> {
    find_instances(api, class, true)
}

/// find_instances with include_inactive=true, returning Option.
pub fn walk(api: &Api<Value>, class: &str) -> Option<Vec<Value>> {
    find_instances(api, class, true).ok()
}
