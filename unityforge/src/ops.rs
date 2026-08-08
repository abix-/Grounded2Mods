//! Unity-side generic primitive op handlers. Register against
//! the shared `modforge::ops::OP_REGISTRY`.
//!
//! Ops shipped here mirror the ueforge surface where the concept
//! maps cleanly, and add Unity-flavored variants where it does
//! not:
//!   - walk_class, inspect_object, list_singletons
//!   - read_field, write_field, invoke_method
//!   - list_ops, list_selectors (auto-generated)

use std::ffi::c_void;

use serde_json::{Value as Json, json};

use modforge::args::{arg_str, arg_u64};
use modforge::ops::{OP_REGISTRY, OpDef};

use crate::bridge::MonoHandle;
use crate::hook::{self, HookCtx};
use crate::main_thread_queue::MAIN_QUEUE;
use crate::mono::{MonoObject, MonoType};

/// Register every framework-shipped Unity op. Game crates call
/// this once at worker init, typically before their own
/// per-game `OP_REGISTRY.register(...)` calls.
pub fn register_builtins() {
    OP_REGISTRY.register_many([
        OpDef::new(
            "ping",
            "Liveness probe",
            "{}",
            |_args| Ok(json!({"pong": true})),
        ),
        OpDef::new(
            "walk_class",
            "Enumerate every live instance of a Mono type",
            "{class: str, include_inactive?: bool}",
            walk_class,
        ),
        OpDef::new(
            "inspect_object",
            "Dump an object's fields + property values",
            "{handle: i32}",
            inspect_object,
        ),
        OpDef::new(
            "read_field",
            "Read a typed field on a Mono object handle",
            "{handle: i32, field: str}",
            read_field,
        ),
        OpDef::new(
            "write_field",
            "Write a typed field on a Mono object handle (main-thread queued)",
            "{handle: i32, field: str, value: any}",
            write_field,
        ),
        OpDef::new(
            "invoke_method",
            "Invoke a method on a Mono object handle (main-thread queued)",
            "{handle: i32, method: str, args?: array}",
            invoke_method,
        ),
        OpDef::new(
            "list_singletons",
            "Get the live Singleton<T>.Instance for each named type",
            "{types: [str]}",
            list_singletons,
        ),
        OpDef::new(
            "harmony_probe",
            "Try a Harmony prefix patch on class.method with a no-op callback, report ok or the failure, then unpatch. Diagnoses per-target patchability without a game restart; the shim logs the full exception to the player log.",
            "{class: str, method: str, ctx_kind?: 0|1}",
            harmony_probe,
        ),
        OpDef::new(
            "release_handle",
            "Free one handle from the shim's handle table. Complex values returned by read_field / invoke_method / inspect_object carry a live handle for chaining; release it when done with a research walk.",
            "{handle: u64}",
            release_handle,
        ),
        OpDef::new(
            "list_ops",
            "Auto-generated catalog of every registered debug op",
            "{}",
            |_args| Ok(OP_REGISTRY.list_json()),
        ),
        OpDef::new(
            "op_metrics",
            "Per-op latency metrics (sorted by total_ns)",
            "{}",
            |_args| Ok(modforge::ops::metrics_json()),
        ),
        OpDef::new(
            "list_selectors",
            "Auto-generated catalog of every registered selector kind",
            "{}",
            |_args| Ok(crate::selector::list_json()),
        ),
        OpDef::new(
            "list_methods",
            "Enumerate methods on a type (walks inheritance chain)",
            "{class: str}",
            list_methods,
        ),
    ]);
}

/// Run `f` on the game's main thread via MAIN_QUEUE and wait for
/// the answer (bounded oneshot). Every op that touches Unity or
/// game code goes through here: on IL2CPP, native calls from the
/// HTTP thread are a process crash, not an exception (seen live
/// 2026-08-07: 0xc0000005 during an off-main inspect_object).
fn on_main<T: Send + 'static>(
    op_name: &str,
    timeout: std::time::Duration,
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    use parking_lot::Mutex;
    use std::sync::Arc;
    let result: Arc<Mutex<Option<T>>> = Arc::new(Mutex::new(None));
    let r2 = result.clone();
    MAIN_QUEUE.push(move || {
        *r2.lock() = Some(f());
    });
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(r) = result.lock().take() {
            return Ok(r);
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!("{op_name}: main-thread queue timed out"));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn walk_class(args: &Json) -> Result<Json, String> {
    let class = arg_str(args, "class")?.to_string();
    let include_inactive = args
        .get("include_inactive")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    on_main("walk_class", std::time::Duration::from_secs(2), move || {
        let ty = MonoType::find(&class).ok_or_else(|| format!("type '{class}' not found"))?;
        ty.walk(include_inactive)
    })?
}

fn list_methods(args: &Json) -> Result<Json, String> {
    use std::ffi::CString;
    let class = arg_str(args, "class")?;
    let bridge = crate::bridge::try_get()?;
    let c_name = CString::new(class).map_err(|e| format!("bad class: {e}"))?;
    let mut buf = vec![0u8; 65536];
    let n = (bridge.list_methods)(
        c_name.as_ptr(),
        buf.as_mut_ptr() as *mut std::os::raw::c_char,
        buf.len() as i32,
    );
    if n < 0 {
        return Err("list_methods: buffer too small".into());
    }
    buf.truncate(n as usize);
    serde_json::from_slice::<Json>(&buf).map_err(|e| format!("list_methods json: {e}"))
}

fn inspect_object(args: &Json) -> Result<Json, String> {
    let h = arg_u64(args, "handle", None)? as i32;
    on_main("inspect_object", std::time::Duration::from_secs(2), move || {
        // SAFETY: caller asserts handle is live; if not, the shim's
        // dictionary lookup returns null and the op surfaces an
        // error.
        let obj = unsafe { MonoObject::from_handle(MonoHandle(h)) };
        let r = obj.dump();
        std::mem::forget(obj); // don't release; caller may reuse the handle
        r
    })?
}

fn read_field(args: &Json) -> Result<Json, String> {
    let h = arg_u64(args, "handle", None)? as i32;
    let field = arg_str(args, "field")?.to_string();
    on_main("read_field", std::time::Duration::from_secs(2), move || {
        let obj = unsafe { MonoObject::from_handle(MonoHandle(h)) };
        let r = obj.read_field(&field);
        std::mem::forget(obj);
        r
    })?
}

fn write_field(args: &Json) -> Result<Json, String> {
    let h = arg_u64(args, "handle", None)? as i32;
    let field = arg_str(args, "field")?.to_string();
    let value = args.get("value").cloned().unwrap_or(Json::Null);
    on_main("write_field", std::time::Duration::from_secs(1), move || {
        let obj = unsafe { MonoObject::from_handle(MonoHandle(h)) };
        let res = obj.write_field(&field, &value);
        std::mem::forget(obj);
        res.map(|_| json!({"written": true}))
    })?
}

extern "C" fn probe_prefix_noop(ctx: *const c_void) -> i32 {
    // ctx-kind probes hand us an owned handle; release it so the
    // probe doesn't leak table entries.
    let h = ctx as isize as i32;
    if h != 0 {
        // SAFETY: the shim's ctx dispatcher acquired this handle
        // for this call and we own it; Drop releases it.
        let obj = unsafe { MonoObject::from_handle(MonoHandle(h)) };
        drop(obj);
    }
    0 // never skip the original
}

fn harmony_probe(args: &Json) -> Result<Json, String> {
    let class = arg_str(args, "class")?.to_string();
    let method = arg_str(args, "method")?.to_string();
    let ctx_kind = args.get("ctx_kind").and_then(Json::as_i64);

    on_main("harmony_probe", std::time::Duration::from_secs(5), move || {
        let res = match ctx_kind {
            Some(0) => hook::patch_prefix_ctx(&class, &method, HookCtx::Instance, probe_prefix_noop),
            Some(1) => hook::patch_prefix_ctx(&class, &method, HookCtx::Arg0, probe_prefix_noop),
            _ => hook::patch_prefix(&class, &method, probe_prefix_noop),
        };
        match res {
            Ok(hook) => {
                drop(hook); // unpatch immediately; the probe only tests applicability
                Ok(json!({"patched": true, "unpatched": true}))
            }
            Err(e) => Err(format!("{e} (full exception in the player log)")),
        }
    })?
}

fn invoke_method(args: &Json) -> Result<Json, String> {
    let h = arg_u64(args, "handle", None)? as i32;
    let method = arg_str(args, "method")?.to_string();
    let m_args = args.get("args").cloned().unwrap_or(Json::Array(vec![]));
    on_main("invoke_method", std::time::Duration::from_secs(2), move || {
        let obj = unsafe { MonoObject::from_handle(MonoHandle(h)) };
        let res = obj.invoke(&method, &m_args);
        std::mem::forget(obj);
        res
    })?
}

fn release_handle(args: &Json) -> Result<Json, String> {
    let h = arg_u64(args, "handle", None)? as i32;
    on_main("release_handle", std::time::Duration::from_secs(1), move || {
        // SAFETY: releasing is what Drop does; a stale handle is a
        // no-op in the shim's table.
        let obj = unsafe { MonoObject::from_handle(MonoHandle(h)) };
        drop(obj);
        Ok(json!({"released": true}))
    })?
}

fn list_singletons(args: &Json) -> Result<Json, String> {
    let types = args
        .get("types")
        .and_then(Json::as_array)
        .cloned()
        .ok_or("missing arg 'types' (array of class names)")?;
    on_main("list_singletons", std::time::Duration::from_secs(2), move || {
        let mut out = Vec::with_capacity(types.len());
        for t in &types {
            let Some(name) = t.as_str() else {
                continue;
            };
            let entry = match MonoType::find(name) {
                None => json!({"class": name, "found": false}),
                Some(ty) => match ty.singleton_instance() {
                    None => json!({"class": name, "found": false, "type_found": true}),
                    Some(obj) => {
                        let h = obj.handle();
                        std::mem::forget(obj); // keep alive; caller may inspect_object
                        json!({"class": name, "found": true, "handle": h.0})
                    }
                },
            };
            out.push(entry);
        }
        Ok(json!({"singletons": out}))
    })?
}
