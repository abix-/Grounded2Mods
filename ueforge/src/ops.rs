//! Generic op handlers. Each takes the JSON `args` blob from a
//! request and returns the JSON `result` blob (or an error string).
//! Snapshot building, the envelope, and the dispatch match arm
//! belong to the embedding crate.
//!
//! Selector-resolving ops accept a closure so the game's
//! game-specific selectors plug in. The convention:
//!
//! ```ignore
//! fn resolve(s: &str) -> Result<&'static UObject, String> {
//!     if let Some(r) = ueforge::selector::resolve_generic(s) {
//!         return r;
//!     }
//!     match s {
//!         "live_player" => ...,
//!         _ => Err(...),
//!     }
//! }
//!
//! match op {
//!     "read_bytes" => ueforge::ops::read_bytes(&args, resolve),
//!     ...
//! }
//! ```

use std::ffi::c_void;
use std::sync::OnceLock;

use serde_json::Value as Json;

use crate::args::{arg_str, arg_u64};
use crate::ue::{self, UObject, fname::FName};

// Generic OpDef + OpRegistry + dispatch + metrics moved into
// modforge::ops during Phase 0b. ueforge re-exports them so
// existing call sites compile unchanged; the UE-specific
// handlers + register_builtins continue to live below.
pub use modforge::ops::{OP_REGISTRY, OpDef, OpHandler, OpRegistry, metrics_json};

/// Register every framework-shipped op that does NOT need
/// per-game context (no tracker, no selector resolver, no PE
/// queue). Game crates call this once at worker init. Typically
/// before their own per-game `OP_REGISTRY.register(...)` calls.
///
/// Registered: `walk_class`, `fname_to_string`, `inspect_address`,
/// `class_outer_samples`, `sample_thread_modules`, scanner ops
/// (`scan_memory` / `scan_rescan` / `scan_session` / `scan_close`,
/// `freeze` / `unfreeze` / `freeze_list`), and `list_ops` itself.
///
/// Per-game ops with captured context (rpg `skill_*`, `call`,
/// `read_bytes`, `write_bytes`, sim ops) are registered by the
/// game crate, not here.
pub fn register_builtins() {
    // Phase 0b: scanner moved to modforge but its freeze op
    // resolves selectors against the engine. Install ueforge's
    // UObject + selector resolver into modforge::scanner before
    // any scanner op fires. Idempotent.
    crate::scanner::install_ue_resolver();

    OP_REGISTRY.register_many([
        OpDef::new(
            "walk_class",
            "Walk a UClass property chain and return the named fields",
            "{class: str}",
            |args| walk_class(args),
        ),
        OpDef::new(
            "walk_class_chain",
            "Walk objects whose class chain contains a name (survives Blueprint reinstancing)",
            "{needle: str, max?: u64}",
            |args| walk_class_chain(args),
        ),
        OpDef::new(
            "class_functions",
            "List the functions on a LIVE class, read off an instance (finds what the startup discovery cache misses)",
            "{class: str}",
            |args| class_functions(args),
        ),
        OpDef::new(
            "fname_to_string",
            "Resolve an FName u64 to its string form",
            "{fname: u64}",
            |args| fname_to_string(args),
        ),
        OpDef::new(
            "inspect_address",
            "Describe the UObject at an address (class + properties + values)",
            "{addr: hex}",
            |args| inspect_address(args),
        ),
        OpDef::new(
            "class_outer_samples",
            "Sample up to k UObjects under class and return their outers",
            "{class: str, k?: u64}",
            |args| {
                let class_name = arg_str(args, "class")?;
                let k = arg_u64(args, "k", Some(20))? as usize;
                Ok(crate::ue::probe::class_outer_samples(class_name, k))
            },
        ),
        OpDef::new(
            "sample_thread_modules",
            "Sample which DLL each thread is executing in over a duration",
            "{duration_ms?: u64, interval_ms?: u64}",
            |args| {
                let duration_ms = arg_u64(args, "duration_ms", Some(30_000))? as u32;
                let interval_ms = arg_u64(args, "interval_ms", Some(100))? as u32;
                Ok(crate::winproc::sample_thread_modules_json(
                    duration_ms,
                    interval_ms,
                ))
            },
        ),
        // Scanner. Cheat-Engine-style memory search + freezes.
        OpDef::new(
            "scan_memory",
            "First-scan: find all addresses holding `value` of `type`",
            "{type: str, value: any}",
            |args| crate::scanner::scan_memory(args),
        ),
        OpDef::new(
            "scan_rescan",
            "Narrow a session by re-reading current values",
            "{session_id: u64, mode: str, value?: any, delta?: any}",
            |args| crate::scanner::scan_rescan(args),
        ),
        OpDef::new(
            "scan_session",
            "Paginate over a scan session's surviving addresses",
            "{session_id: u64, max?: u64, offset?: u64}",
            |args| crate::scanner::scan_session(args),
        ),
        OpDef::new(
            "scan_close",
            "Drop a scan session's state",
            "{session_id: u64}",
            |args| crate::scanner::scan_close(args),
        ),
        OpDef::new(
            "scan_cancel",
            "Abort the in-flight scan_memory / scan_rescan (chunk-boundary check)",
            "{}",
            |args| crate::scanner::scan_cancel(args),
        ),
        OpDef::new(
            "freeze",
            "Hold a value at addr/selector at hz Hz (re-resolves on staleness)",
            "{selector?: str, addr?: hex, offset?: u64, type: str, value: any, hz?: u32}",
            |args| crate::scanner::freeze(args),
        ),
        OpDef::new(
            "unfreeze",
            "Stop a freeze",
            "{addr: hex}",
            |args| crate::scanner::unfreeze(args),
        ),
        OpDef::new(
            "freeze_list",
            "Show every active freeze",
            "{}",
            |args| crate::scanner::freeze_list(args),
        ),
        OpDef::new(
            "discover_data_tables",
            "Every live UDataTable's row schema (cached; pass refresh=true; name= filters to one)",
            "{refresh?: bool, name?: str}",
            |args| {
                let refresh = args.get("refresh").and_then(|v| v.as_bool()).unwrap_or(false);
                let name = args.get("name").and_then(|v| v.as_str());
                Ok(crate::discovery::data_tables_json(refresh, name))
            },
        ),
        OpDef::new(
            "discover_classes",
            "Every UClass + native properties + functions (cached; pass refresh=true; name= filters to one)",
            "{refresh?: bool, name?: str}",
            |args| {
                let refresh = args.get("refresh").and_then(|v| v.as_bool()).unwrap_or(false);
                let name = args.get("name").and_then(|v| v.as_str());
                Ok(crate::discovery::classes_json(refresh, name))
            },
        ),
        OpDef::new(
            "discover_class_detail",
            "Walk one UClass's properties + functions on demand (safe from eager-walk crash)",
            "{name: str}",
            |args| {
                let name = arg_str(args, "name")?;
                Ok(crate::discovery::class_detail_json(name))
            },
        ),
        OpDef::new(
            "discover_struct_detail",
            "Walk one UScriptStruct's fields on demand",
            "{name: str}",
            |args| {
                let name = arg_str(args, "name")?;
                Ok(crate::discovery::struct_detail_json(name))
            },
        ),
        OpDef::new(
            "discover_structs",
            "Every UScriptStruct + field list (cached; pass refresh=true; name= filters to one)",
            "{refresh?: bool, name?: str}",
            |args| {
                let refresh = args.get("refresh").and_then(|v| v.as_bool()).unwrap_or(false);
                let name = args.get("name").and_then(|v| v.as_str());
                Ok(crate::discovery::structs_json(refresh, name))
            },
        ),
        OpDef::new(
            "dump_data_table",
            "Snapshot every row of a UDataTable, decoded per FProperty class",
            "{table_name: str, max_rows?: u64}",
            |args| {
                let table_name = arg_str(args, "table_name")?;
                let max_rows = args.get("max_rows").and_then(|v| v.as_u64()).map(|n| n as usize);
                crate::data_table::snapshot_table(table_name, max_rows)
                    .ok_or_else(|| format!("table '{table_name}' not loaded or has no RowStruct"))
            },
        ),
        OpDef::new(
            "list_data_tables",
            "Enumerate the registered DataTableRegistry \
             (statically-declared catalog; for the runtime-discovered \
             universe use discover_data_tables)",
            "{}",
            |_args| Ok(crate::data_table::list_json()),
        ),
        OpDef::new(
            "tweak_apply",
            "Apply a runtime-declared tweak: captures vanilla per row \
             on first apply, then writes `set` / `multiply` / `add` of \
             the configured value. Re-applies are idempotent (always \
             re-base on captured vanilla).",
            "{table: str, field: str, kind: \"i32\"|\"f32\"|\"u32\", op: \"set\"|\"multiply\"|\"add\", value: number}",
            |args| crate::data_table::tweak_apply_from_args(args),
        ),
        OpDef::new(
            "tweak_list",
            "Every dynamic tweak currently registered across the i32 / \
             f32 / u32 primitive registries. Each entry reports the \
             captured vanilla_count.",
            "{}",
            |_args| Ok(crate::data_table::dynamic_list_json()),
        ),
        OpDef::new(
            "tweak_revert",
            "Revert one specific (table, field) dynamic tweak, OR all \
             of them when args are empty. Removes the matching entry \
             (or all entries) from <DLL_dir>/tweaks.json so the revert \
             survives Ctrl+R. Returns total rows reverted.",
            "{table?: str, field?: str}",
            |args| {
                let table = args.get("table").and_then(|v| v.as_str());
                let field = args.get("field").and_then(|v| v.as_str());
                let (touched, persisted_removed): (usize, usize) = match (table, field) {
                    (Some(t), Some(f)) => {
                        let rows = crate::data_table::dynamic_revert_one(t, f);
                        let removed = crate::data_table::forget_persisted_pub(t, f)
                            .map_err(|e| format!("tweak_revert: persistence: {e}"))?;
                        (rows, if removed { 1 } else { 0 })
                    }
                    (None, None) => {
                        let rows = crate::data_table::dynamic_revert_all();
                        let removed = crate::data_table::forget_persisted_all_pub()
                            .map_err(|e| format!("tweak_revert: persistence: {e}"))?;
                        (rows, removed)
                    }
                    _ => {
                        return Err(
                            "tweak_revert: pass both `table` and `field`, or neither (revert all)"
                                .to_string(),
                        );
                    }
                };
                Ok(serde_json::json!({
                    "rows_reverted": touched,
                    "persisted_removed": persisted_removed,
                }))
            },
        ),
        OpDef::new(
            "tweak_persisted_list",
            "Snapshot of <DLL_dir>/tweaks.json (the on-disk record of \
             every tweak_apply that succeeded). Re-applied at every \
             mod init.",
            "{}",
            |_args| Ok(crate::data_table::persisted_list_json()),
        ),
        OpDef::new(
            "tweak_persisted_load",
            "Re-read <DLL_dir>/tweaks.json into memory. Use after \
             hand-editing the file. Does NOT re-apply; call \
             tweak_persisted_reapply for that.",
            "{}",
            |_args| Ok(crate::data_table::load_persisted_from_disk()),
        ),
        OpDef::new(
            "tweak_persisted_reapply",
            "Re-apply every persisted tweak from the in-memory mirror. \
             Calls discovery before resolving each field. Returns a \
             per-entry status report.",
            "{}",
            |_args| Ok(crate::data_table::reapply_persisted()),
        ),
        OpDef::new(
            "list_row_names",
            "List every row name in a DataTable (no field decoding)",
            "{table_name: str}",
            |args| list_row_names(args),
        ),
        OpDef::new(
            "list_row_fnames",
            "List row names with raw FName keys for a DataTable",
            "{table_name: str}",
            |args| list_row_fnames(args),
        ),
        OpDef::new(
            "inspect_gmalloc",
            "Resolve GMalloc via patternsleuth and dump vtable (read-only)",
            "{}",
            |_args| inspect_gmalloc(),
        ),
        OpDef::new(
            "list_ops",
            "Auto-generated catalog of every registered debug op",
            "{}",
            |_args| Ok(OP_REGISTRY.list_json()),
        ),
        OpDef::new(
            "op_metrics",
            "Per-op latency metrics: calls / errors / total_ns / max_ns / avg_ns (sorted by total_ns)",
            "{}",
            |_args| Ok(crate::ops::metrics_json()),
        ),
        OpDef::new(
            "resolve_offsets",
            "Run patternsleuth's UE resolvers (GUObjectArray + FNamePool + \
             FNameToString) against the host image. Returns image-relative \
             offsets + side-by-side comparison against the configured \
             hardcoded PlatformOffsets so you can verify drift without \
             rebuilding.",
            "{}",
            |args| crate::ue::resolvers::resolve_offsets_op(args),
        ),
        OpDef::new(
            "list_selectors",
            "Auto-generated catalog of every registered selector kind",
            "{}",
            |_args| Ok(crate::selector::SELECTOR_REGISTRY.list_json()),
        ),
    ]);
}

/// Register the resolver-needing ueforge ops. Each game crate
/// supplies its own selector resolver (typically wraps
/// [`crate::selector::resolve_generic`] with extra game names like
/// `live_player:`); the closure captures it.
///
/// Registers: `read_bytes`, `write_bytes`. (Selector-form `freeze`
/// already accepts a resolver internally via the selector module's
/// `resolve_generic` + game's chained dispatch, so it goes through
/// `register_builtins`.)
pub fn register_with_resolver<R>(resolver: R)
where
    R: Fn(&str) -> Result<&'static UObject, String> + Copy + Send + Sync + 'static,
{
    OP_REGISTRY.register_many([
        OpDef::new(
            "read_bytes",
            "Read N bytes from a selector + offset",
            "{selector: str, offset?: u64, length: u64}",
            move |args| read_bytes(args, resolver),
        ),
        OpDef::new(
            "write_bytes",
            "Write hex bytes to a selector + offset",
            "{selector: str, offset?: u64, hex: str}",
            move |args| write_bytes(args, resolver),
        ),
        OpDef::new(
            "tarray_grow",
            "Grow a TArray via GMalloc->Malloc to a larger max capacity",
            "{instance_selector: str, offset: u64, stride: u64, new_max: i32}",
            move |args| tarray_grow(args, resolver),
        ),
    ]);
}

/// Cap on `read_bytes` length / `write_bytes` payload (1 MiB).
/// Sized to comfortably cover any UE struct walk while preventing
/// pathological reads from hanging the listener.
pub const BYTE_OP_CAP: usize = 0x10_0000;

/// When the resolved object's class is known, clamp
/// `offset + length` to `class.properties_size()`. Returns
/// `Err` if the range falls completely outside the class extent.
/// Returns `Ok(())` (no clamp) if the class has no usable size.
/// most likely on raw `addr:0x...` selectors that bypass class
/// resolution.
fn check_object_bounds(obj: &UObject, offset: usize, length: usize) -> Result<(), String> {
    let Some(class) = obj.class() else { return Ok(()) };
    let size = class.properties_size() as usize;
    if size == 0 || size > 0x100_0000 {
        return Ok(());
    }
    let end = offset.checked_add(length).ok_or_else(|| {
        format!("offset 0x{offset:X} + length 0x{length:X} overflows")
    })?;
    if end > size {
        return Err(format!(
            "offset 0x{offset:X} + length 0x{length:X} = 0x{end:X} \
             exceeds instance size 0x{size:X}"
        ));
    }
    Ok(())
}

pub fn read_bytes<F>(args: &Json, resolve: F) -> Result<Json, String>
where
    F: FnOnce(&str) -> Result<&'static UObject, String>,
{
    let selector = arg_str(args, "instance_selector")?.to_string();
    let offset = arg_u64(args, "offset", Some(0))? as usize;
    let length = arg_u64(args, "length", None)? as usize;
    if length > BYTE_OP_CAP {
        return Err(format!("length {length} > 1MB cap"));
    }
    let obj = resolve(&selector)?;
    let is_raw_addr = selector.starts_with("addr:");
    if !is_raw_addr {
        check_object_bounds(obj, offset, length)?;
    }
    let mut out = vec![0u8; length];
    unsafe {
        let base = obj.field_ptr(offset);
        std::ptr::copy_nonoverlapping(base, out.as_mut_ptr(), length);
    }
    Ok(serde_json::json!({
        "selector": selector,
        "offset": format!("0x{offset:X}"),
        "length": length,
        "bytes_hex": hex::encode(&out),
    }))
}

pub fn write_bytes<F>(args: &Json, resolve: F) -> Result<Json, String>
where
    F: FnOnce(&str) -> Result<&'static UObject, String>,
{
    let selector = arg_str(args, "instance_selector")?.to_string();
    let offset = arg_u64(args, "offset", Some(0))? as usize;
    let bytes = hex::decode(arg_str(args, "bytes_hex")?)
        .map_err(|e| format!("bad hex: {e}"))?;
    if bytes.len() > BYTE_OP_CAP {
        return Err(format!("bytes len {} > 1MB cap", bytes.len()));
    }
    let obj = resolve(&selector)?;
    let is_raw_addr = selector.starts_with("addr:");
    if !is_raw_addr {
        check_object_bounds(obj, offset, bytes.len())?;
    }
    unsafe {
        let dst = obj.field_ptr(offset) as *mut u8;
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
    }
    Ok(serde_json::json!({
        "selector": selector,
        "offset": format!("0x{offset:X}"),
        "wrote_bytes": bytes.len(),
    }))
}

/// Walk `GObjects`, return up to `max` non-CDO instances of the
/// named class (CDOs included if `include_cdo: true`). Pure
/// engine traversal. No host hooks needed.
/// Like `walk_class`, but matches by class NAME anywhere in the
/// object's class chain instead of `find_class_fast` + `is_a`.
/// This is the section 22.13 workaround: `is_a` against a cached
/// UClass returns 0 for reinstanced Blueprint classes; the name
/// chain always matches live instances.
/// Functions on a live class, with the parm block size each one
/// expects.
///
/// The startup discovery walk only sees what was loaded at the
/// time, so anything created later (menus, widgets) is missing
/// from it and `discover_class_detail` returns nothing. This
/// reads the class off a live instance instead.
///
/// A Blueprint that reacts to a key names its handler after that
/// key (`InpActEvt_SpaceBar_...`), and a button handler is named
/// after the button, so this listing is usually enough to find
/// the function to call. See misery research.md 26.5.
pub fn class_functions(args: &Json) -> Result<Json, String> {
    let class_name = arg_str(args, "class")?.to_string();
    let ptr = crate::ue::actor::find_object(&class_name, None, false)
        .ok_or_else(|| format!("no live instance of {class_name}"))?;
    // SAFETY: ptr came from this call's GObjects walk.
    let obj = unsafe { &*(ptr as *const UObject) };
    let cls = obj.class().ok_or("instance has no class")?;
    let addr = obj as *const UObject as usize;
    let mut fns = Vec::new();
    for (name, flags) in cls.iter_functions() {
        let entry = match cls.get_function(&class_name, &name) {
            Some(f) => serde_json::json!({
                "name": name,
                "flags": format!("0x{flags:X}"),
                "parms_size": f.parms_size(),
                "num_parms": f.num_parms(),
            }),
            None => serde_json::json!({
                "name": name,
                "flags": format!("0x{flags:X}"),
            }),
        };
        fns.push(entry);
    }
    Ok(serde_json::json!({
        "class": class_name,
        "instance": format!("0x{addr:X}"),
        "instance_selector": format!("addr:0x{addr:X}"),
        "full_name": obj.full_name(),
        "count": fns.len(),
        "functions": fns,
    }))
}

pub fn walk_class_chain(args: &Json) -> Result<Json, String> {
    let needle = arg_str(args, "needle")?.to_string();
    let max = arg_u64(args, "max", Some(256))? as usize;

    let rt = ue::try_runtime().ok_or("ueforge: ue runtime not initialized")?;
    // SAFETY: rt was returned by try_runtime(); the image_base +
    // offsets pair is what runtime init validated.
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return Err("gobjects view invalid".to_string());
    }

    let mut hits = Vec::with_capacity(max);
    let mut total = 0usize;
    for obj in view.iter() {
        if obj.is_default_object() {
            continue;
        }
        if !crate::ue::actor::class_chain_contains(obj, &needle) {
            continue;
        }
        total += 1;
        if hits.len() >= max {
            continue;
        }
        let addr = obj as *const UObject as usize;
        hits.push(serde_json::json!({
            "addr": format!("0x{addr:X}"),
            "addr_selector": format!("addr:0x{addr:X}"),
            "name": obj.name(),
            "full_name": obj.full_name(),
            "is_cdo": false,
        }));
    }
    Ok(serde_json::json!({
        "needle": needle,
        "total": total,
        "returned": hits.len(),
        "instances": hits,
    }))
}

pub fn walk_class(args: &Json) -> Result<Json, String> {
    let class_name = arg_str(args, "class")?.to_string();
    let max = arg_u64(args, "max", Some(256))? as usize;
    let include_cdo = args
        .get("include_cdo")
        .and_then(Json::as_bool)
        .unwrap_or(false);

    let rt = ue::try_runtime().ok_or("ueforge: ue runtime not initialized")?;
    let class = ue::find_class_fast(&class_name)
        .ok_or_else(|| format!("class '{class_name}' not found"))?;
    // SAFETY: rt was returned by try_runtime(), which is set
    // once by detect_and_init from DllMain-adjacent code; the
    // image_base + offsets pair is what runtime init validated.
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return Err("gobjects view invalid".to_string());
    }

    let mut hits = Vec::with_capacity(max);
    let mut total = 0usize;
    for obj in view.iter() {
        if !obj.is_a(class) {
            continue;
        }
        if !include_cdo && obj.is_default_object() {
            continue;
        }
        total += 1;
        if hits.len() >= max {
            continue;
        }
        let addr = obj as *const UObject as usize;
        hits.push(serde_json::json!({
            "addr": format!("0x{addr:X}"),
            "addr_selector": format!("addr:0x{addr:X}"),
            "name": obj.name(),
            "full_name": obj.full_name(),
            "is_cdo": obj.is_default_object(),
        }));
    }
    Ok(serde_json::json!({
        "class": class_name,
        "total": total,
        "returned": hits.len(),
        "instances": hits,
    }))
}

/// Resolve an FName (passed as a u64. The 8 bytes that make up
/// `{ comparison_index: i32, number: u32 }`) to its display
/// string. Useful from tests that walk TMap<FName, ...> bytes
/// and need to show readable keys instead of raw u64s.
///
/// Args: `{ "fname": <u64> }` (decimal or 0x-prefixed hex via JSON).
/// Result: `{ "string": "<name>" }`.
pub fn fname_to_string(args: &Json) -> Result<Json, String> {
    let raw = arg_u64(args, "fname", None)?;
    // SAFETY: FName is an 8-byte { comparison_index: i32,
    // number: u32 } struct; transmute_copy from a u64 with the
    // same little-endian byte layout is well-defined.
    let fname: FName = unsafe { std::mem::transmute_copy(&raw) };
    let rt = ue::try_runtime().ok_or("ueforge: ue runtime not initialized")?;
    // SAFETY: rt.name_resolver is initialized at runtime detect;
    // to_string accepts any FName (including ones whose
    // comparison_index is out of range. It returns a fallback
    // string rather than panicking).
    let s = unsafe { rt.name_resolver.to_string(fname) };
    Ok(serde_json::json!({ "string": s }))
}

/// Walk a class's property chain. Including the super-class
/// chain. Looking for the field that contains
/// `offset_within_instance`. Returns the field's name + the
/// offset-within-the-field (so callers can render
/// `MaxCanStack +0` for an exact hit, or `Colour +0xC` if the
/// target is mid-struct).
fn locate_property(
    class: &UObject,
    offset_in_instance: u32,
) -> Option<(String, u32, u32)> {
    // SAFETY: caller passes a UObject that IS a UClass instance
    // (resolved via find_class_fast); UClass extends UObject in
    // memory layout so the cast is well-defined.
    let mut cur: Option<&ue::UClass> = Some(unsafe {
        &*(class as *const UObject as *const ue::UClass)
    });
    let mut chain_depth = 0;
    while let Some(c) = cur {
        if chain_depth > 16 {
            break;
        }
        // Use the cached property list. Subsequent calls on the
        // same class share an Arc instead of re-walking + re-resolving
        // FName for every property.
        for p in c.cached_native_properties().iter() {
            if offset_in_instance >= p.offset
                && offset_in_instance < p.offset + p.element_size.max(1)
            {
                return Some((p.name.clone(), p.offset, p.element_size));
            }
        }
        cur = c.super_class();
        chain_depth += 1;
    }
    None
}

/// Given a raw memory address, find the UObject (if any) that
/// contains it. Walks GObjects, computes each object's
/// `[base, base + class.properties_size)` range, returns the
/// first match.
///
/// Args: `{ "addr": "0x<hex>" }`.
///
/// Result on hit:
/// ```json
/// {
///   "found": true,
///   "addr": "0x...",
///   "instance_addr": "0x...",
///   "class": "DataTable",
///   "instance_name": "DT_Materials",
///   "instance_full_name": "DataTable /Game/Data/DT_Materials...",
///   "offset_in_instance": "0x48",
///   "instance_size": 624
/// }
/// ```
///
/// On miss: `{ "found": false, "addr": "0x..." }`. Misses are
/// expected for raw allocator buffers, save blobs, and other
/// non-UObject memory the scanner finds.
pub fn inspect_address(args: &Json) -> Result<Json, String> {
    let addr_str = arg_str(args, "addr")?;
    let target = parse_addr(addr_str)?;

    let rt = ue::try_runtime().ok_or("ueforge: ue runtime not initialized")?;
    // SAFETY: rt was returned by try_runtime(), which is set
    // once by detect_and_init from DllMain-adjacent code; the
    // image_base + offsets pair is what runtime init validated.
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return Err("gobjects view invalid".into());
    }

    for obj in view.iter() {
        let base = obj as *const UObject as usize;
        let Some(class) = obj.class() else { continue };
        let size = class.properties_size() as usize;
        if size == 0 || size > 0x100_0000 {
            continue; // sanity: skip absurd sizes
        }
        if target >= base && target < base + size {
            let off = (target - base) as u32;
            let mut result = serde_json::json!({
                "found": true,
                "addr": format!("0x{target:X}"),
                "instance_addr": format!("0x{base:X}"),
                "instance_addr_selector": format!("addr:0x{base:X}"),
                "class": class.as_object().name(),
                "instance_name": obj.name(),
                "instance_full_name": obj.full_name(),
                "offset_in_instance": format!("0x{off:X}"),
                "instance_size": size,
            });
            // Try to name the field via property walk.
            if let Some((name, field_off, field_size)) =
                locate_property(class.as_object(), off)
            {
                let into_field = off - field_off;
                result["field"] = serde_json::json!(name);
                result["field_offset"] = serde_json::json!(format!("0x{field_off:X}"));
                result["field_size"] = serde_json::json!(field_size);
                if into_field > 0 {
                    result["field_inner_offset"] =
                        serde_json::json!(format!("+0x{into_field:X}"));
                }
            }
            return Ok(result);
        }
    }
    Ok(serde_json::json!({
        "found": false,
        "addr": format!("0x{target:X}"),
        "note": "address not within any UObject (raw allocator memory, save blob, or non-UObject struct)",
    }))
}

fn parse_addr(s: &str) -> Result<usize, String> {
    let hex = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    usize::from_str_radix(hex, 16).map_err(|e| format!("bad address '{s}': {e}"))
}

/// Engine portion of the `call` op: resolve the UFunction by
/// `(class_name, function_name)`, invoke `process_event` against
/// `instance` with the given parm bytes (mutated in place by UE
/// for OUT params), and return the post-call buffer as hex.
///
/// Must be called from the game thread. Game-side op handler
/// resolves the instance selector itself (it may be game-specific)
/// and enqueues a closure that calls this on the host's PE queue.
pub fn exec_call(
    instance: &UObject,
    class_name: &str,
    function_name: &str,
    mut parms: Vec<u8>,
) -> Result<Json, String> {
    let class = ue::find_class_fast(class_name)
        .ok_or_else(|| format!("class '{class_name}' not found"))?;
    let func = class.get_function(class_name, function_name).ok_or_else(|| {
        format!("function '{function_name}' not found on '{class_name}'")
    })?;
    // SAFETY: `instance` is a live UObject; `func` is its
    // UFunction returned by get_function (also live in
    // GObjects); `parms.as_mut_ptr()` points at a Vec<u8> buffer
    // owned by this fn for the duration of the call. The engine
    // reads from + writes into the buffer per the UFunction's
    // parm layout (caller is responsible for sizing parms
    // correctly via the framework's parm_size helpers).
    unsafe {
        instance.process_event(func, parms.as_mut_ptr() as *mut c_void);
    }
    Ok(serde_json::json!({
        "parms_hex_after": hex::encode(&parms),
    }))
}

fn list_row_names(args: &Json) -> Result<Json, String> {
    let table_name = arg_str(args, "table_name")?;
    let table = crate::ue::datatable::find_by_short_name(table_name)
        .ok_or_else(|| format!("table '{table_name}' not found"))?;
    let name_map = unsafe { crate::ue::datatable::row_name_map(table) };
    let mut names: Vec<String> = name_map.into_keys().collect();
    names.sort();
    Ok(serde_json::json!({
        "table_name": table_name,
        "count": names.len(),
        "rows": names,
    }))
}

fn list_row_fnames(args: &Json) -> Result<Json, String> {
    let table_name = arg_str(args, "table_name")?;
    let table = crate::ue::datatable::find_by_short_name(table_name)
        .ok_or_else(|| format!("table '{table_name}' not found"))?;
    let name_map = unsafe { crate::ue::datatable::row_name_map(table) };
    let mut rows: Vec<Json> = name_map
        .into_iter()
        .map(|(name, key)| {
            serde_json::json!({
                "name": name,
                "fname_idx": (key & 0xFFFF_FFFF) as u32,
                "fname_num": (key >> 32) as u32,
            })
        })
        .collect();
    rows.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(serde_json::json!({
        "table_name": table_name,
        "count": rows.len(),
        "rows": rows,
    }))
}

static GMALLOC_ADDR: OnceLock<usize> = OnceLock::new();

fn resolve_gmalloc() -> Result<usize, String> {
    if let Some(addr) = GMALLOC_ADDR.get() {
        return Ok(*addr);
    }
    let resolved = crate::ue::resolvers::resolve_image_offsets()
        .map_err(|e| format!("patternsleuth failed: {e}"))?;
    let base = crate::ue::platform::host_image_base();
    let abs = base + resolved.gmalloc;
    crate::log!("GMalloc resolved at {abs:#x}");
    let _ = GMALLOC_ADDR.set(abs);
    Ok(abs)
}

fn inspect_gmalloc() -> Result<Json, String> {
    let gmalloc_global_addr = resolve_gmalloc()?;
    unsafe {
        let fmalloc_ptr = *(gmalloc_global_addr as *const *const u8);
        if fmalloc_ptr.is_null() {
            return Err("GMalloc is null".into());
        }
        let vtable_ptr = *(fmalloc_ptr as *const *const usize);
        let mut slots = Vec::new();
        for i in 0..10 {
            let entry = *vtable_ptr.add(i);
            slots.push(format!("{entry:#x}"));
        }
        Ok(serde_json::json!({
            "gmalloc_global": format!("{gmalloc_global_addr:#x}"),
            "fmalloc_ptr": format!("{:#x}", fmalloc_ptr as usize),
            "vtable_ptr": format!("{:#x}", vtable_ptr as usize),
            "vtable_slots": slots,
        }))
    }
}

fn tarray_grow<F>(args: &Json, resolve: F) -> Result<Json, String>
where
    F: FnOnce(&str) -> Result<&'static UObject, String>,
{
    let selector = arg_str(args, "instance_selector")?.to_string();
    let offset = arg_u64(args, "offset", None)? as usize;
    let stride = arg_u64(args, "stride", None)? as usize;
    let new_max = arg_u64(args, "new_max", None)? as i32;

    if new_max <= 0 || new_max > 1024 {
        return Err(format!("new_max {new_max} out of range (1..1024)"));
    }

    let obj = resolve(&selector)?;
    let header_ptr = unsafe { obj.field_ptr(offset) };

    unsafe {
        let old_ptr = *(header_ptr as *const *mut u8);
        let old_num = *((header_ptr as usize + 8) as *const i32);
        let old_max = *((header_ptr as usize + 12) as *const i32);

        crate::ue::tarray::grow_raw(header_ptr, stride, new_max)?;

        let new_ptr = *(header_ptr as *const *mut u8);
        Ok(serde_json::json!({
            "old_ptr": format!("{:#x}", old_ptr as u64),
            "new_ptr": format!("{:#x}", new_ptr as u64),
            "old_max": old_max,
            "new_max": new_max,
            "num": old_num,
            "stride": stride,
        }))
    }
}
