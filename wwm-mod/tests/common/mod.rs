//! Shared helpers for the wwm-mod research tests.
//!
//! WWM is Unity Mono, so class names are plain (no Il2Cpp
//! prefix) and every op chains on the shim's object handles.
//! New research tests reuse these; do not copy them into a
//! test file.
//!
//! Each --test target compiles this module separately and no
//! target uses every helper, so dead_code is expected here.
#![allow(dead_code)]

use modforge::client::Api;
use serde_json::{Value, json};

/// Control plane port. Override with WWM_MOD_PORT.
pub fn api() -> Api<Value> {
    let port = std::env::var("WWM_MOD_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(17172);
    // walk_class runs FindObjectsOfType on the main thread; in a
    // loaded world that can take seconds, and a short timeout
    // reads back as "class gone".
    Api::at(port, "/op").with_timeout(std::time::Duration::from_secs(30))
}

/// Ping; on connection failure print a SKIP line and return None
/// so the test passes without a running game.
pub fn ping_or_skip(api: &Api<Value>) -> Option<()> {
    match api.try_op("ping", json!({})) {
        Ok(r) if r.ok => Some(()),
        Ok(r) => panic!("ping not ok: {:?}", r.error),
        Err(e) => {
            eprintln!(
                "SKIP: no control plane answering ({e}); launch WWM with wwm_mod loaded"
            );
            None
        }
    }
}

/// walk_class; Err carries the failure text so callers can tell
/// "type not found" apart from a timeout or a transport error.
/// Conflating those two makes every survival verdict a lie.
pub fn try_walk(api: &Api<Value>, class: &str) -> Result<Vec<Value>, String> {
    // include_inactive: the release parks its managers on
    // inactive GameObjects, so the default scan finds nothing.
    // The shim answers {class, instances: [...]}, not a bare array.
    match api.try_op("walk_class", json!({"class": class, "include_inactive": true})) {
        Ok(r) if r.ok => Ok(r.result["instances"]
            .as_array()
            .cloned()
            .unwrap_or_default()),
        Ok(r) => Err(r.error.unwrap_or_else(|| "op not ok, no error text".into())),
        Err(e) => Err(format!("transport: {e}")),
    }
}

/// True only when the failure text says the type itself is absent.
pub fn is_type_not_found(err: &str) -> bool {
    err.contains("not found")
}

/// walk_class; None when the call did not succeed, for callers
/// that do not care why.
pub fn walk(api: &Api<Value>, class: &str) -> Option<Vec<Value>> {
    try_walk(api, class).ok()
}

/// Walk the class, return the first live instance handle.
/// Prints why it failed: "no live instance" and "the walk never
/// completed" are different problems and look identical to the
/// caller otherwise.
pub fn first_handle(api: &Api<Value>, class: &str) -> Option<i64> {
    match try_walk(api, class) {
        Ok(instances) => {
            let h = instances.first().and_then(|i| i["handle"].as_i64());
            if h.is_none() {
                println!("{class}: resolves, zero live instances");
            }
            h
        }
        Err(e) => {
            println!("{class}: walk failed ({e})");
            None
        }
    }
}

/// Handle carried by a complex value. The shim attaches one to
/// every object it serializes so ops chain without a selector.
pub fn handle_of(v: &Value) -> Option<i64> {
    v["handle"].as_i64()
}

/// list_methods, filtered to the methods the class itself
/// declares. Unfiltered output is ~250 inherited MonoBehaviour /
/// UnityEngine.Object entries and drowns the real surface.
pub fn print_declared_methods(api: &Api<Value>, class: &str) {
    let r = api.op("list_methods", json!({"class": class}));
    if !r.ok {
        println!("list_methods({class}) failed: {:?}", r.error);
        return;
    }
    let empty = vec![];
    let methods = r.result["methods"].as_array().unwrap_or(&empty);
    for m in methods {
        if m["declared_on"].as_str() == Some(class) {
            println!(
                "  {}({}) -> {}{}",
                m["name"].as_str().unwrap_or("?"),
                m["params"].as_i64().unwrap_or(0),
                m["return"].as_str().unwrap_or("?"),
                if m["static"].as_bool() == Some(true) { " [static]" } else { "" },
            );
        }
    }
}

/// Field name -> value map from inspect_object, or None when the
/// handle is dead / the op fails.
pub fn fields(api: &Api<Value>, handle: i64) -> Option<Value> {
    let r = api.op("inspect_object", json!({"handle": handle}));
    if r.ok { Some(r.result) } else { None }
}

/// True when inspect_object on a live instance of `class` lists
/// `field`. Prints the verdict either way.
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
