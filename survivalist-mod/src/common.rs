//! Shared bridge helpers for the survivalist modules (war,
//! growth, development). Extracted at the third consumer.

use serde_json::{Value as Json, json};
use unityforge::bridge::MonoHandle;
use unityforge::mono::{MonoObject, MonoType};

/// Wrap a handle we own; Drop releases it back to the shim table.
///
/// SAFETY: caller asserts the handle came fresh out of a bridge
/// response (read_field / invoke / ctx dispatcher) and is not
/// wrapped anywhere else.
pub fn own(h: i32) -> MonoObject {
    unsafe { MonoObject::from_handle(MonoHandle(h)) }
}

pub fn handle_of(v: &Json) -> Option<i32> {
    v.get("handle").and_then(Json::as_i64).map(|h| h as i32)
}

pub fn community_manager() -> Result<MonoObject, String> {
    let session = MonoType::find("Session")
        .and_then(|t| t.singleton_instance())
        .ok_or("Session.Instance not found (no game loaded?)")?;
    let cm_h = handle_of(&session.read_field("CommunityManager")?)
        .ok_or("Session.CommunityManager is null")?;
    Ok(own(cm_h))
}

/// Visit every community. `f` takes OWNERSHIP of each wrapper:
/// dropping it releases the handle; `std::mem::forget` keeps the
/// handle alive for use after the loop. Returns true to keep
/// iterating.
pub fn for_each_community(
    mut f: impl FnMut(MonoObject) -> Result<bool, String>,
) -> Result<(), String> {
    let cm = community_manager()?;
    let list_h = handle_of(&cm.read_field("Communities")?).ok_or("Communities list is null")?;
    let list = own(list_h);
    let count = list
        .invoke("get_Count", &json!([]))?
        .as_i64()
        .ok_or("get_Count did not return a number")?;
    for i in 0..count {
        let Some(item_h) = handle_of(&list.invoke("get_Item", &json!([i]))?) else {
            continue;
        };
        if !f(own(item_h))? {
            break;
        }
    }
    Ok(())
}

pub fn display_name(com: &MonoObject) -> String {
    com.invoke("GetDisplayNameString", &json!([]))
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "<unnamed>".to_string())
}

pub fn ctype(com: &MonoObject) -> String {
    com.read_field("CommunityType")
        .map(|v| v.as_str().unwrap_or("?").to_string())
        .unwrap_or_else(|_| "?".to_string())
}

pub fn list_len(owner: &MonoObject, field: &str) -> i64 {
    match owner.read_field(field).ok().as_ref().and_then(handle_of) {
        Some(h) => own(h)
            .invoke("get_Count", &json!([]))
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        None => 0,
    }
}

/// Read an (x, y) pair from either the shim's struct-object form
/// (`{"x": .., "y": ..}`, the current bridge) or the legacy
/// ToString form ("(x, y)").
pub fn parse_xy(v: &Json) -> Option<(f32, f32)> {
    if let Some(o) = v.as_object() {
        let g = |k: &str| o.get(k).and_then(Json::as_f64).map(|f| f as f32);
        if let (Some(x), Some(y)) = (g("x"), g("y")) {
            return Some((x, y));
        }
    }
    let s = v.as_str()?;
    let s = s.trim().trim_start_matches('(').trim_end_matches(')');
    let mut it = s.split(',');
    let x = it.next()?.trim().parse::<f32>().ok()?;
    let y = it.next()?.trim().parse::<f32>().ok()?;
    Some((x, y))
}

pub fn pos_of(obj: &MonoObject) -> Option<(f32, f32)> {
    if let Ok(v) = obj.read_field("PosXZ") {
        if let Some(p) = parse_xy(&v) {
            return Some(p);
        }
    }
    obj.read_field("Tile").ok().and_then(|v| parse_xy(&v))
}

/// Run `f` on the Unity main thread and wait for its result
/// (same oneshot shape as unityforge's write_field op).
pub fn on_main_thread<F>(f: F) -> Result<Json, String>
where
    F: FnOnce() -> Result<Json, String> + Send + 'static,
{
    use std::sync::Arc;

    use parking_lot::Mutex;
    let result: Arc<Mutex<Option<Result<Json, String>>>> = Arc::new(Mutex::new(None));
    let r2 = result.clone();
    unityforge::main_thread_queue::MAIN_QUEUE.push(move || {
        *r2.lock() = Some(f());
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(r) = result.lock().take() {
            return r;
        }
        if std::time::Instant::now() >= deadline {
            return Err("op: main-thread queue timed out".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
