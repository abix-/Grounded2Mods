//! Mono reflection wrappers. Each type is a thin idiomatic Rust
//! handle that calls through the bridge table.
//!
//! Handles are 32-bit cookies (not pointers). The C# shim owns
//! the actual managed reference in a `Dictionary<int, object>`.
//! Dropping a Rust wrapper releases the handle back to the shim.

use std::ffi::{CStr, CString};
use std::mem::ManuallyDrop;

use serde_json::Value as Json;

use crate::bridge::{self, MonoHandle};

/// An owned reference to a Mono `Type`. Resolves via
/// `bridge::mono_find_type` and releases on Drop.
pub struct MonoType {
    handle: MonoHandle,
}

impl MonoType {
    /// Look up a type by its (fully-qualified) name. Returns
    /// `None` if the type is not loaded or no shim is installed.
    pub fn find(name: &str) -> Option<Self> {
        let bridge = bridge::get()?;
        let c = CString::new(name).ok()?;
        let handle = (bridge.find_type)(c.as_ptr());
        if handle.is_null() {
            None
        } else {
            Some(Self { handle })
        }
    }

    pub fn handle(&self) -> MonoHandle {
        self.handle
    }

    /// `Singleton<T>.Instance` for this type, when the type is
    /// derived from the game's `Singleton<T>` template.
    pub fn singleton_instance(&self) -> Option<MonoObject> {
        let bridge = bridge::get()?;
        let h = (bridge.singleton_instance)(self.handle);
        if h.is_null() {
            None
        } else {
            Some(MonoObject { handle: h })
        }
    }

    /// `StaticInstance<T>.Instance` for this type.
    pub fn static_instance(&self) -> Option<MonoObject> {
        let bridge = bridge::get()?;
        let h = (bridge.static_instance)(self.handle);
        if h.is_null() {
            None
        } else {
            Some(MonoObject { handle: h })
        }
    }

    /// Enumerate every live instance as JSON (no Rust-side
    /// MonoObject allocation; downstream code parses the array).
    pub fn walk(&self, include_inactive: bool) -> Result<Json, String> {
        let bridge = bridge::try_get()?;
        let mut buf = vec![0u8; 64 * 1024];
        let n = (bridge.walk_class)(
            self.handle,
            if include_inactive { 1 } else { 0 },
            buf.as_mut_ptr() as *mut _,
            buf.len() as i32,
        );
        if n < 0 {
            return Err(format!(
                "mono_walk_class: buffer too small (cap={})",
                buf.len()
            ));
        }
        buf.truncate(n as usize);
        let s = std::str::from_utf8(&buf).map_err(|e| format!("walk: bad utf-8: {e}"))?;
        serde_json::from_str(s).map_err(|e| format!("walk: bad json: {e}"))
    }
}

impl Drop for MonoType {
    fn drop(&mut self) {
        if let Some(bridge) = bridge::get() {
            (bridge.release_handle)(self.handle);
        }
    }
}

/// An owned reference to a Mono `object` instance.
pub struct MonoObject {
    handle: MonoHandle,
}

impl MonoObject {
    /// Take ownership of a managed handle returned by the bridge.
    pub fn from_owned_handle(handle: MonoHandle) -> Self {
        Self { handle }
    }

    /// Wrap an existing handle without taking ownership. Used by
    /// op handlers that get a handle by address-selector and want
    /// to operate on it for the duration of one call.
    ///
    /// SAFETY: the caller asserts the handle is live; this
    /// wrapper will call `mono_release_handle` on Drop. If two
    /// wrappers exist for the same handle, the second release is
    /// a no-op (the shim's Dictionary.Remove returns false).
    pub unsafe fn from_handle(handle: MonoHandle) -> Self {
        Self { handle }
    }

    pub fn handle(&self) -> MonoHandle {
        self.handle
    }

    /// Invoke `List<T>.get_Count` on a managed collection.
    fn list_count(&self) -> Result<Json, String> {
        self.invoke("get_Count", &serde_json::json!([]))
    }

    /// Invoke `List<T>.get_Item` on a managed collection.
    fn list_item(&self, index: i64) -> Result<Json, String> {
        self.invoke("get_Item", &serde_json::json!([index]))
    }

    /// Read a managed collection count as an integer.
    pub fn list_len(&self) -> Result<i64, String> {
        self.list_count()?
            .as_i64()
            .ok_or_else(|| "get_Count did not return a number".to_string())
    }

    /// Read a managed collection count, treating a non-numeric
    /// bridge value as the empty-list default used by game mods.
    pub fn list_len_or_zero(&self) -> Result<i64, String> {
        Ok(self.list_count()?.as_i64().unwrap_or(0))
    }

    /// Read one managed collection entry as a handle.
    pub fn list_handle(&self, index: i64) -> Result<Option<i32>, String> {
        Ok(json_handle(&self.list_item(index)?))
    }

    /// Count the managed list stored in `field`, returning zero for
    /// a missing field, null list, or bridge failure.
    pub fn field_list_len(&self, field: &str) -> i64 {
        self.read_field(field)
            .ok()
            .as_ref()
            .and_then(json_handle)
            .map(owned_object)
            .and_then(|list| list.list_len().ok())
            .unwrap_or(0)
    }

    /// Read a typed field. Returns the JSON-tagged value.
    pub fn read_field(&self, field: &str) -> Result<Json, String> {
        let bridge = bridge::try_get()?;
        let c = CString::new(field).map_err(|e| format!("bad field name: {e}"))?;
        let mut buf = vec![0u8; 4096];
        let n = (bridge.read_field)(
            self.handle,
            c.as_ptr(),
            buf.as_mut_ptr() as *mut _,
            buf.len() as i32,
        );
        if n < 0 {
            return Err(format!("read_field '{field}': not found or buf too small"));
        }
        buf.truncate(n as usize);
        let s = std::str::from_utf8(&buf).map_err(|e| format!("read_field: bad utf-8: {e}"))?;
        serde_json::from_str(s).map_err(|e| format!("read_field: bad json: {e}"))
    }

    /// Write a typed field. `value` is JSON in the same tagged
    /// shape as read_field.
    pub fn write_field(&self, field: &str, value: &Json) -> Result<(), String> {
        let bridge = bridge::try_get()?;
        let field_c = CString::new(field).map_err(|e| format!("bad field name: {e}"))?;
        let value_s =
            serde_json::to_string(value).map_err(|e| format!("write_field: bad value: {e}"))?;
        let value_c = CString::new(value_s).map_err(|e| format!("bad value: {e}"))?;
        let r = (bridge.write_field)(self.handle, field_c.as_ptr(), value_c.as_ptr());
        match r {
            0 => Ok(()),
            -1 => Err(format!("write_field '{field}': type mismatch")),
            -2 => Err(format!("write_field '{field}': not found")),
            n => Err(format!("write_field '{field}': unexpected code {n}")),
        }
    }

    /// Invoke a method. `args` is a JSON array.
    pub fn invoke(&self, method: &str, args: &Json) -> Result<Json, String> {
        let bridge = bridge::try_get()?;
        let method_c = CString::new(method).map_err(|e| format!("bad method: {e}"))?;
        let args_s = serde_json::to_string(args).map_err(|e| format!("bad args: {e}"))?;
        let args_c = CString::new(args_s).map_err(|e| format!("bad args: {e}"))?;
        let mut buf = vec![0u8; 8192];
        let r = (bridge.invoke_method)(
            self.handle,
            method_c.as_ptr(),
            args_c.as_ptr(),
            buf.as_mut_ptr() as *mut _,
            buf.len() as i32,
        );
        let len = if r >= 0 {
            r as usize
        } else {
            // -3: shim wrote {"error":...} to buf; surface it.
            // For -1/-2 there's no payload; use the code.
            if r == -3 {
                let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
                let s = std::str::from_utf8(&buf[..end])
                    .map_err(|e| format!("invoke: bad utf-8 on error path: {e}"))?;
                return Err(s.to_string());
            }
            return Err(match r {
                -1 => format!("invoke '{method}': not found"),
                -2 => format!("invoke '{method}': arg mismatch"),
                _ => format!("invoke '{method}': unexpected code {r}"),
            });
        };
        let end = len.min(buf.len());
        let s = std::str::from_utf8(&buf[..end]).map_err(|e| format!("invoke: bad utf-8: {e}"))?;
        serde_json::from_str(s).map_err(|e| format!("invoke: bad json: {e}"))
    }

    /// Inspect-all-fields helper for the `inspect_object` op.
    pub fn dump(&self) -> Result<Json, String> {
        let bridge = bridge::try_get()?;
        let mut buf = vec![0u8; 32 * 1024];
        let n = (bridge.inspect_object)(
            self.handle,
            buf.as_mut_ptr() as *mut _,
            buf.len() as i32,
        );
        if n < 0 {
            return Err("inspect_object: buffer too small".into());
        }
        buf.truncate(n as usize);
        let s = std::str::from_utf8(&buf).map_err(|e| format!("inspect: bad utf-8: {e}"))?;
        serde_json::from_str(s).map_err(|e| format!("inspect: bad json: {e}"))
    }
}

/// Decode a managed-object handle from a bridge JSON value.
pub fn json_handle(value: &Json) -> Option<i32> {
    value.get("handle").and_then(Json::as_i64).map(|h| h as i32)
}

/// Take ownership of a raw managed handle returned by the bridge.
pub fn owned_object(handle: i32) -> MonoObject {
    MonoObject::from_owned_handle(MonoHandle(handle))
}

/// Borrow a managed handle for one operation without releasing it.
pub fn with_object<R>(handle: i32, f: impl FnOnce(&MonoObject) -> R) -> R {
    let object = ManuallyDrop::new(owned_object(handle));
    f(&object)
}

/// Decode a Unity two-dimensional value from the bridge object
/// form or its legacy `(x, y)` string form.
pub fn unity_xy(value: &Json) -> Option<(f32, f32)> {
    if let Some(object) = value.as_object() {
        let component = |key: &str| {
            object
                .get(key)
                .and_then(Json::as_f64)
                .map(|number| number as f32)
        };
        if let (Some(x), Some(y)) = (component("x"), component("y")) {
            return Some((x, y));
        }
    }
    let text = value.as_str()?;
    let text = text.trim().trim_start_matches('(').trim_end_matches(')');
    let mut components = text.split(',');
    let x = components.next()?.trim().parse::<f32>().ok()?;
    let y = components.next()?.trim().parse::<f32>().ok()?;
    Some((x, y))
}

impl Drop for MonoObject {
    fn drop(&mut self) {
        if let Some(bridge) = bridge::get() {
            (bridge.release_handle)(self.handle);
        }
    }
}

/// Invoke a STATIC method on a named class (bridge v6+). On a
/// pre-v6 shim (loaded before the upgrade; the game has not been
/// restarted yet) this returns Err naming the restart.
pub fn invoke_static(class: &str, method: &str, args: &Json) -> Result<Json, String> {
    let bridge = bridge::try_get()?;
    let Some(f) = bridge.invoke_static else {
        return Err(format!(
            "invoke_static '{class}.{method}': the running shim is pre-v6; restart the game to load the upgraded shim"
        ));
    };
    let class_c = CString::new(class).map_err(|e| format!("bad class: {e}"))?;
    let method_c = CString::new(method).map_err(|e| format!("bad method: {e}"))?;
    let args_s = serde_json::to_string(args).map_err(|e| format!("bad args: {e}"))?;
    let args_c = CString::new(args_s).map_err(|e| format!("bad args: {e}"))?;
    let mut buf = vec![0u8; 8192];
    let r = f(
        class_c.as_ptr(),
        method_c.as_ptr(),
        args_c.as_ptr(),
        buf.as_mut_ptr() as *mut _,
        buf.len() as i32,
    );
    let len = if r >= 0 {
        r as usize
    } else {
        if r == -3 {
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            let s = std::str::from_utf8(&buf[..end])
                .map_err(|e| format!("invoke_static: bad utf-8 on error path: {e}"))?;
            return Err(s.to_string());
        }
        return Err(match r {
            -1 => format!("invoke_static '{class}.{method}': class or method not found"),
            -2 => format!("invoke_static '{class}.{method}': arg mismatch"),
            _ => format!("invoke_static '{class}.{method}': unexpected code {r}"),
        });
    };
    let end = len.min(buf.len());
    let s = std::str::from_utf8(&buf[..end]).map_err(|e| format!("invoke_static: bad utf-8: {e}"))?;
    serde_json::from_str(s).map_err(|e| format!("invoke_static: bad json: {e}"))
}

/// Convenience: log a line through the shim's BepInEx sink.
pub fn log(level: LogLevel, msg: &str) {
    let Some(bridge) = bridge::get() else { return };
    let Ok(c) = CString::new(msg) else { return };
    (bridge.log_emit)(level as i32, c.as_ptr());
}

#[repr(i32)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

/// Read a UTF-8 C string from a raw pointer. Helper used by
/// patch-callback trampolines.
///
/// SAFETY: `ptr` must point to a NUL-terminated UTF-8 string for
/// the duration of the borrow.
pub unsafe fn cstr_to_str<'a>(ptr: *const std::os::raw::c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: caller asserts ptr is a NUL-terminated UTF-8 string
    // valid for the duration of the borrow.
    unsafe { CStr::from_ptr(ptr).to_str().ok() }
}
