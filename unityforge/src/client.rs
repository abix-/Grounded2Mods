//! Blocking test client for Unity mods.
//!
//! Re-exports everything from `modforge::client` and adds
//! Unity-specific research helpers (IL2CPP name fallback,
//! include_inactive scans).

pub use modforge::client::*;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

/// Find all live instances of a Unity class. Tries the `Il2Cpp`
/// prefixed name first (IL2CPP games), then the plain name.
///
/// `include_inactive`: when true, the shim scans inactive
/// GameObjects too (some managers park on inactive objects).
pub fn find_instances<S: DeserializeOwned>(
    api: &Api<S>,
    class: &str,
    include_inactive: bool,
) -> Result<Vec<Value>, String> {
    for name in [format!("Il2Cpp{class}"), class.to_string()] {
        let r = match api.try_op(
            "walk_class",
            json!({"class": name, "include_inactive": include_inactive}),
        ) {
            Ok(r) if r.ok => r,
            Ok(r) => {
                let err = r.error.unwrap_or_else(|| "op not ok".into());
                println!("find_instances({name}) failed: {err}");
                continue;
            }
            Err(e) => return Err(format!("transport: {e}")),
        };

        let instances = r
            .result
            .get("instances")
            .and_then(Value::as_array)
            .cloned()
            .or_else(|| r.result.as_array().cloned())
            .unwrap_or_default();

        println!("find_instances({name}): {} instance(s)", instances.len());
        return Ok(instances);
    }
    Err(format!("{class}: neither Il2Cpp{class} nor {class} resolved"))
}

/// Find the first live instance handle of a class.
pub fn first_handle<S: DeserializeOwned>(api: &Api<S>, class: &str) -> Option<i64> {
    first_handle_opts(api, class, false)
}

/// Find the first live instance handle, scanning inactive objects too.
pub fn first_handle_inactive<S: DeserializeOwned>(
    api: &Api<S>,
    class: &str,
) -> Option<i64> {
    first_handle_opts(api, class, true)
}

fn first_handle_opts<S: DeserializeOwned>(
    api: &Api<S>,
    class: &str,
    include_inactive: bool,
) -> Option<i64> {
    match find_instances(api, class, include_inactive) {
        Ok(instances) => {
            let h = instances.first().and_then(|i| handle_of(i));
            if h.is_none() {
                println!("{class}: resolves, zero live instances");
            }
            h
        }
        Err(e) => {
            println!("{class}: find_instances failed ({e})");
            None
        }
    }
}

/// True when inspect_object on a live instance of `class` lists
/// `field`. Prints the verdict either way.
pub fn field_exists<S: DeserializeOwned>(
    api: &Api<S>,
    class: &str,
    field: &str,
) -> bool {
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
