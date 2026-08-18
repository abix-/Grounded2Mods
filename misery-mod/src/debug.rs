//! HTTP control plane. Same shape as outworld-station-mod's, and
//! starts just as empty: ops and snapshot fields grow as the
//! emission research lands.

use std::sync::OnceLock;

use serde::Serialize;
use serde_json::Value as Json;

use ueforge::envelope::{OpResponse as UespyResponse, parse_request};

/// Register every misery-mod op + selector into the workspace
/// registries. Called once before the HTTP listener starts.
fn register_ops() {
    ueforge::selector::register_builtins();
    ueforge::ops::register_builtins();
    ueforge::ops::register_with_resolver(ueforge::selector::resolve);

    ueforge::ops::OP_REGISTRY.register(ueforge::ops::OpDef::new(
        "list_row_names",
        "List every row name in a DataTable (no field decoding)",
        "{table_name: str}",
        |args| list_row_names(args),
    ));

    ueforge::ops::OP_REGISTRY.register(ueforge::ops::OpDef::new(
        "list_row_fnames",
        "List row names with raw FName keys for a DataTable",
        "{table_name: str}",
        |args| list_row_fnames(args),
    ));

    ueforge::ops::OP_REGISTRY.register(ueforge::ops::OpDef::new(
        "inspect_gmalloc",
        "Resolve GMalloc via patternsleuth and dump vtable (read-only)",
        "{}",
        |args| inspect_gmalloc(args),
    ));

    ueforge::ops::OP_REGISTRY.register(ueforge::ops::OpDef::new(
        "tarray_grow",
        "Grow a TArray via GMalloc->Malloc to a larger max capacity",
        "{instance_selector: str, offset: u64, stride: u64, new_max: i32}",
        |args| tarray_grow(args),
    ));
}

fn list_row_names(args: &Json) -> Result<Json, String> {
    let table_name = args["table_name"]
        .as_str()
        .ok_or("missing arg 'table_name'")?;
    let table = ueforge::ue::datatable::find_by_short_name(table_name)
        .ok_or_else(|| format!("table '{table_name}' not found"))?;
    let name_map = unsafe { ueforge::ue::datatable::row_name_map(table) };
    let mut names: Vec<String> = name_map.into_keys().collect();
    names.sort();
    Ok(serde_json::json!({
        "table_name": table_name,
        "count": names.len(),
        "rows": names,
    }))
}

fn list_row_fnames(args: &Json) -> Result<Json, String> {
    let table_name = args["table_name"]
        .as_str()
        .ok_or("missing arg 'table_name'")?;
    let table = ueforge::ue::datatable::find_by_short_name(table_name)
        .ok_or_else(|| format!("table '{table_name}' not found"))?;
    let name_map = unsafe { ueforge::ue::datatable::row_name_map(table) };
    let mut rows: Vec<Json> = name_map.into_iter()
        .map(|(name, key)| serde_json::json!({
            "name": name,
            "fname_idx": (key & 0xFFFF_FFFF) as u32,
            "fname_num": (key >> 32) as u32,
        }))
        .collect();
    rows.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(serde_json::json!({
        "table_name": table_name,
        "count": rows.len(),
        "rows": rows,
    }))
}

/// Cached absolute address of the GMalloc global.
static GMALLOC_ADDR: OnceLock<usize> = OnceLock::new();

fn resolve_gmalloc() -> Result<usize, String> {
    if let Some(addr) = GMALLOC_ADDR.get() {
        return Ok(*addr);
    }
    let resolved = ueforge::ue::resolvers::resolve_image_offsets()
        .map_err(|e| format!("patternsleuth failed: {e}"))?;
    let base = ueforge::ue::platform::host_image_base();
    let abs = base + resolved.gmalloc;
    ueforge::log::log(format_args!("GMalloc resolved at {abs:#x}"));
    let _ = GMALLOC_ADDR.set(abs);
    Ok(abs)
}

/// Read-only: resolve GMalloc and dump the vtable for inspection.
fn inspect_gmalloc(_args: &Json) -> Result<Json, String> {
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

fn tarray_grow(args: &Json) -> Result<Json, String> {
    let selector = args["instance_selector"]
        .as_str()
        .ok_or("missing arg 'instance_selector'")?;
    let offset = args["offset"]
        .as_u64()
        .ok_or("missing arg 'offset'")?;
    let stride = args["stride"]
        .as_u64()
        .ok_or("missing arg 'stride'")?;
    let new_max = args["new_max"]
        .as_i64()
        .ok_or("missing arg 'new_max'")? as i32;

    if new_max <= 0 || new_max > 1024 {
        return Err(format!("new_max {new_max} out of range (1..1024)"));
    }

    let obj = ueforge::selector::resolve(selector)?;
    let header_ptr = unsafe { obj.field_ptr(offset as usize) };

    unsafe {
        let old_ptr = *(header_ptr as *const *mut u8);
        let old_num = *((header_ptr as usize + 8) as *const i32);
        let old_max = *((header_ptr as usize + 12) as *const i32);

        ueforge::ue::tarray::grow_raw(header_ptr, stride as usize, new_max)?;

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

pub fn spawn(port: u16) {
    register_ops();
    ueforge::spawn(
        ueforge::Config {
            port,
            endpoint: "/debug",
            thread_name: "misery-mod-debug",
            auth_token: None,
        },
        |body| {
            let resp = handle(body);
            serde_json::to_vec(&resp).unwrap_or_else(|_| b"{}".to_vec())
        },
        |msg| ueforge::log::log(format_args!("{msg}")),
    );
}

/// Game-specific snapshot fields. `offsets_known` is the one that
/// matters right now: until the platform offsets are filled in,
/// every object-walking op will fail and this says why.
#[derive(Serialize, Default)]
pub struct Snapshot {
    pub offsets_known: bool,
}

type OpResponse = UespyResponse<Snapshot>;

fn build_snapshot() -> Snapshot {
    Snapshot {
        offsets_known: ueforge::ue::try_runtime().is_some(),
    }
}

fn handle(body: &str) -> OpResponse {
    let (op, args) = match parse_request(body) {
        Ok(v) => v,
        Err(e) => return error_response("<parse-error>", e),
    };

    if op == "snapshot" {
        return ok_response(&op, Json::Null);
    }
    if op.is_empty() {
        return error_response(
            "<missing>",
            "missing 'op' field; try op:'list_ops' for the catalog",
        );
    }
    match ueforge::ops::OP_REGISTRY.dispatch(&op, &args) {
        Some(r) => to_response(&op, r),
        None => error_response(
            &op,
            format!("unknown op '{op}'; try op:'list_ops' for the catalog"),
        ),
    }
}

fn ok_response(op: &str, result: Json) -> OpResponse {
    OpResponse::ok(op, result, build_snapshot())
}

fn to_response(op: &str, r: Result<Json, String>) -> OpResponse {
    OpResponse::from_result(op, r, build_snapshot())
}

fn error_response(op: &str, err: impl Into<String>) -> OpResponse {
    OpResponse::err(op, err, build_snapshot())
}
