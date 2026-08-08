//! Shared helpers for the schedule1-mod research tests.
//!
//! The control plane is generic (handle chaining): these helpers
//! are the whole research toolkit. New research tests reuse them;
//! do not copy them into a test file.
//!
//! Each --test target compiles this module separately and no
//! target uses every helper, so dead_code is expected here.
#![allow(dead_code)]

use modforge::client::Api;
use serde_json::{Value, json};

pub fn api() -> Api<Value> {
    let port = std::env::var("SCHEDULE1_MOD_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(17175);
    Api::at(port, "/op")
}

/// Ping; on connection failure print a SKIP line and return None
/// so the test passes without a running game.
pub fn ping_or_skip(api: &Api<Value>) -> Option<()> {
    match api.try_op("ping", json!({})) {
        Ok(r) if r.ok => Some(()),
        Ok(r) => panic!("ping not ok: {:?}", r.error),
        Err(e) => {
            eprintln!("SKIP: no control plane answering ({e}); launch the game with schedule1_mod loaded");
            None
        }
    }
}

/// walk_class trying the interop name first, then the plain name.
pub fn walk(api: &Api<Value>, class: &str) -> Option<Vec<Value>> {
    for name in [format!("Il2Cpp{class}"), class.to_string()] {
        let walk = api.op("walk_class", json!({"class": name}));
        if walk.ok {
            let instances = walk.result.as_array().cloned().unwrap_or_default();
            println!("walk_class({name}): {} instance(s)", instances.len());
            return Some(instances);
        }
        println!("walk_class({name}) failed: {:?}", walk.error);
    }
    None
}

/// Walk the class, return the first live instance handle.
pub fn first_handle(api: &Api<Value>, class: &str) -> Option<i64> {
    let instances = walk(api, class)?;
    let handle = instances.first().and_then(|i| i["handle"].as_i64());
    if handle.is_none() {
        println!("{class}: resolvable but zero live instances (scene-dependent?)");
    }
    handle
}

/// Handle carried by a complex value (attached by the shim's
/// serializer so ops chain generically).
pub fn handle_of(v: &Value) -> Option<i64> {
    v.get("handle").and_then(Value::as_i64)
}

/// Element count of any Il2Cpp sequence: arrays answer
/// get_Length, lists answer get_Count.
pub fn count_of(api: &Api<Value>, h: i64) -> Option<i64> {
    for getter in ["get_Length", "get_Count"] {
        let r = api.op("invoke_method", json!({"handle": h, "method": getter, "args": []}));
        if r.ok {
            return r.result.as_i64();
        }
    }
    None
}

/// Walk a sequence handle generically: get_Item(i) per element,
/// inspect each element, print its fields, release the handles.
pub fn dump_sequence(api: &Api<Value>, label: &str, seq: i64) {
    let Some(n) = count_of(api, seq) else {
        println!("{label}: no get_Length/get_Count answered");
        return;
    };
    println!("{label}: {n} element(s)");
    for i in 0..n {
        let item = api.op("invoke_method", json!({"handle": seq, "method": "get_Item", "args": [i]}));
        if !item.ok {
            println!("{label}[{i}]: get_Item failed: {:?}", item.error);
            continue;
        }
        let Some(eh) = handle_of(&item.result) else {
            println!("{label}[{i}] = {}", item.result);
            continue;
        };
        let inspect = api.op("inspect_object", json!({"handle": eh}));
        println!("{label}[{i}]:\n{}",
            serde_json::to_string_pretty(&inspect.result).unwrap_or_default());
        api.op("release_handle", json!({"handle": eh}));
    }
    api.op("release_handle", json!({"handle": seq}));
}

/// Compact method catalog: one "name(params) -> return" line per
/// method the class declares (inherited entries filtered by the
/// declared_on field matching the queried class).
pub fn print_declared_methods(api: &Api<Value>, class: &str) {
    let r = api.op("list_methods", json!({"class": class}));
    if !r.ok {
        println!("list_methods({class}) failed: {:?}", r.error);
        return;
    }
    let methods = r.result["methods"].as_array().cloned().unwrap_or_default();
    println!("{class} declares:");
    for m in &methods {
        if m["declared_on"].as_str() != Some(class) {
            continue;
        }
        println!(
            "  {}({}) -> {}{}",
            m["name"].as_str().unwrap_or("?"),
            m["params"].as_i64().unwrap_or(-1),
            m["return"].as_str().unwrap_or("?"),
            if m["static"].as_bool() == Some(true) { " [static]" } else { "" },
        );
    }
}
