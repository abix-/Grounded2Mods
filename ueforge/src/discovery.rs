//! Reflection-discovery snapshot: one GObjects pass produces a
//! cached view of every live `UDataTable`, `UClass`, and
//! `UScriptStruct` in the game. The mod runs this at load and
//! every consumer (debug ops, ImGui browser, future TweakDef
//! resolution) reads from the cache.
//!
//! ```text
//! K8s slot: Def=DiscoverySnapshot (singleton; no kinds. Carve-out
//!                                  like Settings),
//!           Registry=n/a,
//!           Instance=cached Arc<DiscoverySnapshot>,
//!           Controller=run_at_load / refresh
//! ```
//!
//! Why in-memory, not files on disk:
//!
//! - **DRY.** The HTTP `discover_*` ops + the ImGui browser tab
//!   read from the same cache. The walk runs ONCE per refresh.
//! - **Always current.** A game patch that shifts an offset is
//!   reflected the next time anything reads from the cache; no
//!   stale-JSON-on-disk class of bug.
//! - **Cheap on hot reload.** Mod reload doesn't have to clean up
//!   files; the cache disappears with the DLL.
//!
//! If you want a file, redirect a curl: `curl ... > dump.json`.
//!
//! ## Usage
//!
//! ```ignore
//! // From the game crate's on_unreal_init worker, after
//! // ue::platform::detect_and_init:
//! let _ = ueforge::discovery::run_at_load();
//! // logs e.g. "discovery: 412 data_tables, 28341 classes, 1893 structs"
//! ```
//!
//! ## Re-walking
//!
//! GObjects grows as content streams in (level loads, save loads,
//! level transitions). Call [`refresh`] (or hit any `discover_*`
//! op with `{refresh: true}`) to re-walk; the new snapshot
//! replaces the old in the cache atomically.

use std::sync::{Arc, OnceLock};

use parking_lot::RwLock;
use serde_json::{Value as Json, json};

use crate::ue::{self, UClass, UObject};

/// One snapshot of the game's reflection state. Created by [`walk`],
/// stored in the global cache by [`run_at_load`] / [`refresh`].
pub struct DiscoverySnapshot {
    pub data_tables: Json,
    pub classes: Json,
    pub structs: Json,
}

static CACHE: OnceLock<RwLock<Option<Arc<DiscoverySnapshot>>>> = OnceLock::new();

fn cell() -> &'static RwLock<Option<Arc<DiscoverySnapshot>>> {
    CACHE.get_or_init(|| RwLock::new(None))
}

// ---- Walk instrumentation ------------------------------------------------
//
// Per-object trace logging is disabled (was used to hunt an OWS
// GObjects-walk crash; the SEH wrapper in `NameResolver` + the
// FString cap fixed the root cause). Flip `TRACE_ENABLED` to true
// + tighten the range here to re-enable for future investigations.
const TRACE_ENABLED: bool = false;
const TRACE_FROM: usize = 19_000;
const TRACE_TO: usize = 30_000;

#[inline]
fn trace_step(scanned: usize, addr: usize, step: &str) {
    if TRACE_ENABLED && scanned >= TRACE_FROM && scanned < TRACE_TO {
        crate::log::log(format_args!("dscv[{scanned}] addr=0x{addr:x} step={step}"));
    }
}

#[inline]
fn trace_step_str(scanned: usize, label: &str, step: &str) {
    if TRACE_ENABLED && scanned >= TRACE_FROM && scanned < TRACE_TO {
        crate::log::log(format_args!("dscv[{scanned}] {step}={label}"));
    }
}

/// Currently cached snapshot, if [`run_at_load`] / [`refresh`] has
/// run at least once.
pub fn cached() -> Option<Arc<DiscoverySnapshot>> {
    cell().read().clone()
}

/// Walk GObjects once, populate the cache, log a one-line summary.
/// Returns the new snapshot for the caller (typically discarded).
pub fn run_at_load() -> Arc<DiscoverySnapshot> {
    let snap = Arc::new(walk());
    *cell().write() = Some(snap.clone());
    crate::log::log(format_args!(
        "discovery: {} data_tables, {} classes, {} structs",
        snap.data_tables["tables_found"].as_u64().unwrap_or(0),
        snap.classes["classes_found"].as_u64().unwrap_or(0),
        snap.structs["structs_found"].as_u64().unwrap_or(0),
    ));
    snap
}

/// Re-walk and replace the cache. Same as [`run_at_load`] today;
/// kept distinct so future versions can diff against the prior
/// snapshot for "what new content loaded" reporting.
pub fn refresh() -> Arc<DiscoverySnapshot> {
    run_at_load()
}

/// JSON view: every live `UDataTable` + row schema. Refresh-on-demand
/// for op handlers. If the cache is cold, walks now. `name_filter`
/// restricts the result to a single entry by exact short name (the
/// table's UE short name); pass `None` for the full dump.
pub fn data_tables_json(refresh_first: bool, name_filter: Option<&str>) -> Json {
    let snap = snapshot(refresh_first);
    match name_filter {
        None => snap.data_tables.clone(),
        Some(name) => filter_named(&snap.data_tables, "data_tables", "table_name", name),
    }
}

/// JSON view: every `UClass` + native property list + function list.
pub fn classes_json(refresh_first: bool, name_filter: Option<&str>) -> Json {
    let snap = snapshot(refresh_first);
    match name_filter {
        None => snap.classes.clone(),
        Some(name) => filter_named(&snap.classes, "classes", "name", name),
    }
}

/// JSON view: every `UScriptStruct` + field list.
pub fn structs_json(refresh_first: bool, name_filter: Option<&str>) -> Json {
    let snap = snapshot(refresh_first);
    match name_filter {
        None => snap.structs.clone(),
        Some(name) => filter_named(&snap.structs, "structs", "name", name),
    }
}

/// Wrap one entry from a discovery section back in the section's
/// envelope (same shape minus the count fields, plus a `match`
/// pointer to the located entry).
fn filter_named(section: &Json, array_key: &str, name_key: &str, target: &str) -> Json {
    let empty = Vec::new();
    let arr = section
        .get(array_key)
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    let found = arr
        .iter()
        .find(|e| e.get(name_key).and_then(|v| v.as_str()) == Some(target));
    match found {
        Some(e) => json!({
            "match": e.clone(),
            "filter": target,
        }),
        None => json!({
            "match": null,
            "filter": target,
            "error": format!("{} '{}' not in discovery cache", name_key, target),
        }),
    }
}

fn snapshot(refresh_first: bool) -> Arc<DiscoverySnapshot> {
    if refresh_first {
        return refresh();
    }
    if let Some(s) = cached() {
        return s;
    }
    run_at_load()
}

fn walk() -> DiscoverySnapshot {
    let Some(rt) = ue::try_runtime() else {
        return error_snap("ueforge: ue runtime not initialized");
    };
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return error_snap("gobjects view invalid");
    }
    let dt_class = ue::find_class_fast("DataTable");

    let mut data_tables: Vec<Json> = Vec::new();
    let mut classes: Vec<Json> = Vec::new();
    let mut structs: Vec<Json> = Vec::new();
    let mut scanned = 0usize;
    let mut panicked = 0usize;
    let mut unreadable = 0usize;

    crate::log::log(format_args!(
        "discovery: starting GObjects walk ({} entries reported)",
        view.num()
    ));

    for obj in view.iter() {
        scanned += 1;
        // Coarse progress log every 1000. 175 lines max per
        // walk. The danger zone in OWS (20K-40K) gets per-object
        // step traces; see `trace_step` calls below.
        if scanned.is_multiple_of(1000) {
            crate::log::log(format_args!(
                "discovery: progress {scanned} scanned, {} tables, {} classes, {} structs",
                data_tables.len(),
                classes.len(),
                structs.len(),
            ));
        }
        let obj_raw = obj as *const UObject as usize;
        trace_step(scanned, obj_raw, "enter");
        // Guard against bad UObject pointers in GObjects (freed
        // entries, never-fully-constructed stubs). We need to be
        // able to read at least through OUTER (+0x20) for the
        // class+name+outer chain. Bail before dereferencing if
        // the page isn't committed.
        let obj_addr = obj as *const UObject as usize;
        if !crate::winproc::is_addr_readable(obj_addr)
            || !crate::winproc::is_addr_readable(obj_addr + crate::ue::offsets::uobject::OUTER)
        {
            unreadable += 1;
            continue;
        }
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            trace_step(scanned, obj_raw, "class()");
            let Some(cls) = obj.class() else { return };
            let cls_addr = cls as *const UClass as usize;
            trace_step(scanned, cls_addr, "cls_addr_check");
            if !crate::winproc::is_addr_readable(cls_addr) {
                return;
            }
            trace_step(scanned, cls_addr, "cls_name");
            let cls_name = cls.as_object().name();
            trace_step_str(scanned, &cls_name, "after_cls_name");

            if let Some(dt) = dt_class {
                trace_step(scanned, obj_raw, "is_a(DataTable)");
                if obj.is_a(dt) {
                    trace_step(scanned, obj_raw, "is_default_object");
                    if !obj.is_default_object() {
                        trace_step(scanned, obj_raw, "describe_data_table");
                        data_tables.push(crate::ue::probe::describe_data_table(obj));
                    }
                }
            }

            if cls_name == "Class" {
                trace_step(scanned, obj_raw, "describe_class");
                let uclass: &UClass = unsafe { &*(obj as *const UObject as *const UClass) };
                classes.push(describe_class(uclass));
            } else if cls_name == "ScriptStruct" {
                trace_step(scanned, obj_raw, "describe_script_struct");
                structs.push(describe_script_struct(obj));
            }
            trace_step(scanned, obj_raw, "exit_ok");
        }));
        if res.is_err() {
            panicked += 1;
        }
    }
    if panicked > 0 {
        crate::log::log(format_args!(
            "discovery: {panicked} per-object panics caught (objects skipped)"
        ));
    }
    if unreadable > 0 {
        crate::log::log(format_args!(
            "discovery: {unreadable} unreadable UObject entries skipped"
        ));
    }

    let data_tables_found = data_tables.len();
    let classes_found = classes.len();
    let structs_found = structs.len();

    DiscoverySnapshot {
        data_tables: json!({
            "data_tables": data_tables,
            "scanned_objects": scanned,
            "tables_found": data_tables_found,
        }),
        classes: json!({
            "classes": classes,
            "scanned_objects": scanned,
            "classes_found": classes_found,
        }),
        structs: json!({
            "structs": structs,
            "scanned_objects": scanned,
            "structs_found": structs_found,
        }),
    }
}

/// Minimal eager describe. Just the short name; `full_path` /
/// `super` / property + function chains move to
/// [`class_detail_json`] on-demand. Eager walks of 175K+ objects
/// in OWS hit corrupt outer-chain pointers when `obj.full_name()`
/// walks more than one level deep, so we stay shallow here.
fn describe_class(cls: &UClass) -> Json {
    let obj = cls.as_object();
    json!({ "name": obj.name() })
}

fn describe_script_struct(obj: &UObject) -> Json {
    json!({ "name": obj.name() })
}

/// On-demand: walk one class's native property + function chains.
/// Used by `discover_class_detail` op + class browser when the
/// user clicks Open. Guarded reads inside `iter_native_properties` /
/// `iter_functions` keep a bad chain on one class from taking the
/// host down.
pub fn class_detail_json(class_name: &str) -> Json {
    let Some(cls) = crate::ue::find_class_fast(class_name) else {
        return json!({"error": format!("class '{class_name}' not loaded")});
    };
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let props = cls.cached_native_properties();
        let fields: Vec<Json> = props
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "offset": p.offset,
                    "element_size": p.element_size,
                })
            })
            .collect();
        let functions: Vec<Json> = cls
            .iter_functions()
            .into_iter()
            .map(|(name, flags)| {
                json!({
                    "name": name,
                    "function_flags": format!("0x{:08x}", flags),
                })
            })
            .collect();
        json!({
            "name": class_name,
            "properties_size": cls.properties_size(),
            "fields": fields,
            "functions": functions,
        })
    }));
    match res {
        Ok(j) => j,
        Err(_) => json!({"error": format!("panic while describing '{class_name}'")}),
    }
}

/// On-demand: walk one UScriptStruct's field chain. Used by the
/// struct browser + `discover_struct_detail` op.
pub fn struct_detail_json(struct_name: &str) -> Json {
    let Some(rt) = crate::ue::try_runtime() else {
        return json!({"error": "ueforge: ue runtime not initialized"});
    };
    let view = unsafe { crate::ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return json!({"error": "gobjects view invalid"});
    }
    for obj in view.iter() {
        let obj_addr = obj as *const UObject as usize;
        if !crate::winproc::is_addr_readable(obj_addr)
            || !crate::winproc::is_addr_readable(obj_addr + crate::ue::offsets::uobject::OUTER)
        {
            continue;
        }
        // Wrap the per-object class+name reads in catch_unwind.
        // FName resolve can panic on corrupt FName indices in
        // late-loaded content, especially when scanning 175K objects.
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let cls = obj.class()?;
            let cls_addr = cls as *const crate::ue::UClass as usize;
            if !crate::winproc::is_addr_readable(cls_addr) {
                return None;
            }
            if cls.as_object().name() != "ScriptStruct" {
                return None;
            }
            if obj.name() != struct_name {
                return None;
            }
            Some(())
        }));
        if !matches!(res, Ok(Some(()))) {
            continue;
        }
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let fields = crate::ue::probe::walk_struct_fields(obj);
            json!({
                "name": struct_name,
                "full_path": obj.full_name(),
                "fields": fields,
            })
        }));
        return match res {
            Ok(j) => j,
            Err(_) => json!({"error": format!("panic while describing '{struct_name}'")}),
        };
    }
    json!({"error": format!("struct '{struct_name}' not found")})
}

fn error_snap(msg: &str) -> DiscoverySnapshot {
    DiscoverySnapshot {
        data_tables: json!({"error": msg}),
        classes: json!({"error": msg}),
        structs: json!({"error": msg}),
    }
}
