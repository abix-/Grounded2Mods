//! Block the WWM demo-end panel.
//!
//! Calls the framework's `list_methods` bridge fn to enumerate
//! every method declared anywhere on the inheritance chain of
//! `DemoCompleteScreenUI`. For each candidate that looks like
//! a show/open/lifecycle entry point (or its `Update`), installs
//! a Harmony prefix that returns non-zero so HarmonyLib skips
//! the original. The result is that the demo-complete screen
//! never wires up its input handler. The user keeps playing.
//!
//! The method names on this class were the missing piece in
//! prior attempts; with `list_methods` we no longer guess.

use std::ffi::{CString, c_void};

use serde_json::Value as Json;
use unityforge::hook::{HOOK_REGISTRY, patch_prefix};

const TYPE_NAME: &str = "DemoCompleteScreenUI";

pub fn install() {
    let methods = match enumerate_methods(TYPE_NAME) {
        Ok(m) => m,
        Err(e) => {
            unityforge::mono::log(
                unityforge::mono::LogLevel::Warn,
                &format!("wwm-mod: list_methods({TYPE_NAME}) failed: {e}"),
            );
            return;
        }
    };

    unityforge::mono::log(
        unityforge::mono::LogLevel::Info,
        &format!(
            "wwm-mod: {} declares {} method(s):",
            TYPE_NAME,
            methods.len()
        ),
    );
    for m in &methods {
        unityforge::mono::log(
            unityforge::mono::LogLevel::Info,
            &format!(
                "  - {}::{}({} args, static={}, returns {})",
                m.declared_on, m.name, m.params, m.is_static, m.ret
            ),
        );
    }

    // Patch any method declared on DemoCompleteScreenUI itself.
    // Skip statics + property getters/setters; their roles are
    // either obvious no-ops or we'd never want to block them.
    let mut blocked: Vec<String> = Vec::new();
    for m in &methods {
        if m.declared_on != TYPE_NAME {
            continue;
        }
        if m.is_static {
            continue;
        }
        if m.name.starts_with("get_") || m.name.starts_with("set_") {
            continue;
        }
        match patch_prefix(TYPE_NAME, &m.name, on_blocked_prefix) {
            Ok(hook) => {
                HOOK_REGISTRY.register(hook);
                blocked.push(m.name.clone());
            }
            Err(e) => {
                unityforge::mono::log(
                    unityforge::mono::LogLevel::Warn,
                    &format!("wwm-mod: patch_prefix({}, {}) err: {e}", TYPE_NAME, m.name),
                );
            }
        }
    }
    if blocked.is_empty() {
        unityforge::mono::log(
            unityforge::mono::LogLevel::Warn,
            "wwm-mod: no patchable methods found on DemoCompleteScreenUI",
        );
    } else {
        unityforge::mono::log(
            unityforge::mono::LogLevel::Info,
            &format!(
                "wwm-mod: blocked {} method(s): {}",
                blocked.len(),
                blocked.join(", ")
            ),
        );
    }
}

extern "C" fn on_blocked_prefix(_ctx: *const c_void) -> i32 {
    unityforge::mono::log(
        unityforge::mono::LogLevel::Info,
        "wwm-mod: skipped DemoCompleteScreenUI method (panel blocked)",
    );
    1
}

struct MethodInfo {
    name: String,
    declared_on: String,
    params: i64,
    is_static: bool,
    ret: String,
}

fn enumerate_methods(type_name: &str) -> Result<Vec<MethodInfo>, String> {
    let bridge = unityforge::bridge::try_get()?;
    let c_name = CString::new(type_name).map_err(|e| format!("bad name: {e}"))?;
    let mut buf = vec![0u8; 65536];
    let n = (bridge.list_methods)(
        c_name.as_ptr(),
        buf.as_mut_ptr() as *mut std::os::raw::c_char,
        buf.len() as i32,
    );
    if n < 0 {
        return Err("buffer too small".into());
    }
    buf.truncate(n as usize);
    let v: Json = serde_json::from_slice(&buf).map_err(|e| e.to_string())?;
    if let Some(err) = v.get("error").and_then(Json::as_str) {
        return Err(err.to_string());
    }
    let arr = v
        .get("methods")
        .and_then(Json::as_array)
        .ok_or("no methods field")?;
    let mut out = Vec::with_capacity(arr.len());
    for m in arr {
        out.push(MethodInfo {
            name: m
                .get("name")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            declared_on: m
                .get("declared_on")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            params: m.get("params").and_then(Json::as_i64).unwrap_or(0),
            is_static: m.get("static").and_then(Json::as_bool).unwrap_or(false),
            ret: m
                .get("return")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
        });
    }
    Ok(out)
}
