//! Generic research-test helpers for the debug-endpoint client.
//!
//! Every UE5 mod investigation follows the same handful of
//! patterns: find a DataTable, walk its rows, find class
//! instances by name, resolve FNames to strings, read field
//! bytes at known offsets. The `explore_*` tests in g2rpg /
//! ows-tweaks all reimplemented these from scratch. ~150-250
//! lines per test of mostly-boilerplate.
//!
//! This module collapses the boilerplate into one-liners. A
//! per-game research test now looks like:
//!
//! ```ignore
//! use ueforge::client::research;
//!
//! let api = common::Api::require();
//! let (dt_sel, dt_addr) = research::find_data_table_by_name(&api, "DT_Materials")
//!     .expect("DT_Materials not found");
//! let rows = research::read_data_table_rows(&api, &dt_sel)?;
//! for row in rows {
//!     let name = research::fname_to_string(&api, row.fname).unwrap_or_default();
//!     let stack = research::read_i32(&api, row.addr, 0x48);
//!     eprintln!("  {name:<40} MaxCanStack={stack}");
//! }
//! ```
//!
//! The shared helpers all go through the existing op set
//! (`walk_class`, `read_bytes`, `fname_to_string`); no new
//! mod-side surface needed.

use serde::de::DeserializeOwned;
use serde_json::json;

use super::Api;

// UDataTable.RowMap (TMap<FName, uint8*>) at +0x30 on UE 5.x.
// Mirror of `ueforge::ue::offsets::datatable::ROW_MAP`.
const ROW_MAP_OFFSET: u64 = 0x30;
// TMap layout (UE 5.x): { void* Data; i32 Num; i32 Max }.
const TMAP_HEADER_BYTES: u64 = 16;
// TSparseArray<TSetElement<TPair<K, V>>> stride = 24 bytes
// (8-byte FName key + 8-byte row pointer + i32 HashNextId +
// i32 HashIndex). Mirror of `ueforge::ue::offsets::tmap::ELEMENT_SIZE`.
const TMAP_ELEMENT_SIZE: usize = 24;

/// One row in a DataTable's RowMap.
#[derive(Debug, Clone, Copy)]
pub struct DtRow {
    /// Row name (FName u64). Pass to [`fname_to_string`] for the
    /// human-readable short name. Pass directly to a
    /// `FDataTableRowHandle`-shaped UFunction parm to invoke
    /// engine code that operates on this row.
    pub fname: u64,
    /// Row body's UObject address. Use `addr:0x{addr:X}` as
    /// the `instance_selector` for follow-up `read_bytes` /
    /// `write_bytes` ops.
    pub addr: u64,
}

impl DtRow {
    /// Format `addr` as the `addr:0x...` selector string.
    pub fn addr_selector(&self) -> String {
        format!("addr:0x{:X}", self.addr)
    }
}

/// One UClass instance returned by [`walk_class_instances`].
#[derive(Debug, Clone)]
pub struct ClassInstance {
    /// `addr:0x...` selector (use directly in op args).
    pub addr_selector: String,
    /// Raw address (UObject*).
    pub addr: u64,
    /// Short name (UObject::GetName).
    pub name: String,
    /// Fully-qualified path name.
    pub full_name: String,
}

/// Find a UDataTable whose short name (`UObject::GetName`)
/// matches exactly. Returns `(addr_selector, raw_addr)` or
/// `None` if not loaded yet.
pub fn find_data_table_by_name<S: DeserializeOwned>(
    api: &Api<S>,
    short_name: &str,
) -> Option<(String, u64)> {
    let r = api.op(
        "walk_class",
        json!({"class": "DataTable", "max": 10000, "include_cdo": false}),
    );
    if !r.ok {
        return None;
    }
    let inst = r.result.get("instances")?.as_array()?.iter().find(|i| {
        i.get("name").and_then(|v| v.as_str()) == Some(short_name)
    })?;
    let sel = inst.get("addr_selector")?.as_str()?.to_string();
    let addr = inst
        .get("addr")
        .and_then(|v| v.as_str())
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())?;
    Some((sel, addr))
}

/// Find a UDataTable whose full path name contains
/// `path_substring`. Use when the short name is ambiguous
/// (e.g. multiple tables named `Table_StatusEffects` exist
/// across asset paths).
pub fn find_data_table_by_path<S: DeserializeOwned>(
    api: &Api<S>,
    path_substring: &str,
) -> Option<(String, u64)> {
    let r = api.op(
        "walk_class",
        json!({"class": "DataTable", "max": 10000, "include_cdo": false}),
    );
    if !r.ok {
        return None;
    }
    let inst = r.result.get("instances")?.as_array()?.iter().find(|i| {
        i.get("full_name")
            .and_then(|v| v.as_str())
            .map(|s| s.contains(path_substring))
            .unwrap_or(false)
    })?;
    let sel = inst.get("addr_selector")?.as_str()?.to_string();
    let addr = inst
        .get("addr")
        .and_then(|v| v.as_str())
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())?;
    Some((sel, addr))
}

/// Walk a `UDataTable`'s `RowMap` and return every live row.
///
/// Implementation: reads the TMap header at +0x30
/// `(data_ptr, num, max)`, then batch-reads the entire element
/// array in one `read_bytes` call (`num * 24` bytes). Skips
/// slots whose row pointer is null (sparse-array free slots).
pub fn read_data_table_rows<S: DeserializeOwned>(
    api: &Api<S>,
    table_addr_selector: &str,
) -> Result<Vec<DtRow>, String> {
    // 1. TMap header.
    let header_resp = api.op(
        "read_bytes",
        json!({
            "instance_selector": table_addr_selector,
            "offset": ROW_MAP_OFFSET,
            "length": TMAP_HEADER_BYTES,
        }),
    );
    if !header_resp.ok {
        return Err(format!(
            "read_bytes header on {table_addr_selector} failed: {:?}",
            header_resp.error
        ));
    }
    let bytes = hex::decode(
        header_resp
            .result
            .get("bytes_hex")
            .and_then(|v| v.as_str())
            .ok_or("header response missing bytes_hex")?,
    )
    .map_err(|e| format!("hex decode: {e}"))?;
    if bytes.len() < 16 {
        return Err(format!("header too short: {} bytes", bytes.len()));
    }
    let data_ptr = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    let data_num = i32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if data_num <= 0 || data_ptr == 0 {
        return Ok(Vec::new());
    }

    // 2. Element array.
    let total_bytes = (data_num as u64) * (TMAP_ELEMENT_SIZE as u64);
    let elem_resp = api.op(
        "read_bytes",
        json!({
            "instance_selector": format!("addr:0x{data_ptr:X}"),
            "offset": 0,
            "length": total_bytes,
        }),
    );
    if !elem_resp.ok {
        return Err(format!(
            "read_bytes element-array failed: {:?}",
            elem_resp.error
        ));
    }
    let slot_bytes = hex::decode(
        elem_resp
            .result
            .get("bytes_hex")
            .and_then(|v| v.as_str())
            .ok_or("element response missing bytes_hex")?,
    )
    .map_err(|e| format!("hex decode: {e}"))?;

    let mut rows = Vec::with_capacity(data_num as usize);
    for i in 0..(data_num as usize) {
        let off = i * TMAP_ELEMENT_SIZE;
        if off + 16 > slot_bytes.len() {
            break;
        }
        let fname = u64::from_le_bytes(slot_bytes[off..off + 8].try_into().unwrap());
        let addr = u64::from_le_bytes(slot_bytes[off + 8..off + 16].try_into().unwrap());
        if addr == 0 {
            continue; // sparse-array free slot
        }
        rows.push(DtRow { fname, addr });
    }
    Ok(rows)
}

/// Resolve a packed FName u64 to a human-readable short name via
/// the `fname_to_string` op. None if the op fails.
pub fn fname_to_string<S: DeserializeOwned>(api: &Api<S>, fname: u64) -> Option<String> {
    let r = api.op("fname_to_string", json!({"fname": fname}));
    if !r.ok {
        return None;
    }
    r.result.get("string").and_then(|v| v.as_str()).map(String::from)
}

/// Walk every instance of a UClass and return a list with
/// `addr_selector` + name + full_name. `max` caps the walk; pass
/// a large value (e.g. 10000) for full enumeration.
pub fn walk_class_instances<S: DeserializeOwned>(
    api: &Api<S>,
    class: &str,
    max: usize,
) -> Vec<ClassInstance> {
    walk_class_inner(api, class, max, false)
}

/// Like [`walk_class_instances`] but includes CDOs (objects
/// where `is_cdo == true`). Use [`find_class_cdo`] if you only
/// want the CDO.
pub fn walk_class_instances_with_cdo<S: DeserializeOwned>(
    api: &Api<S>,
    class: &str,
    max: usize,
) -> Vec<ClassInstance> {
    walk_class_inner(api, class, max, true)
}

/// Find the CDO (class default object) for `class`. Most classes
/// have exactly one CDO. Returns `None` if the class isn't loaded
/// or somehow has no CDO.
pub fn find_class_cdo<S: DeserializeOwned>(
    api: &Api<S>,
    class: &str,
) -> Option<ClassInstance> {
    let r = api.op(
        "walk_class",
        json!({"class": class, "max": 32, "include_cdo": true}),
    );
    if !r.ok {
        return None;
    }
    let arr = r.result.get("instances")?.as_array()?;
    let cdo = arr.iter().find(|i| {
        i.get("is_cdo")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    })?;
    parse_class_instance(cdo)
}

/// Find the first live (non-CDO) instance of `class`. Symmetric
/// to [`find_class_cdo`]. Returns `None` if the class isn't
/// loaded or has no live instances yet.
///
/// Common use: "find the player" / "find the singleton GameMode" /
/// "find the loaded map". Pass the BP class name + take the
/// first non-CDO hit.
pub fn find_live_instance<S: DeserializeOwned>(
    api: &Api<S>,
    class: &str,
) -> Option<ClassInstance> {
    let r = api.op(
        "walk_class",
        json!({"class": class, "max": 32, "include_cdo": false}),
    );
    if !r.ok {
        return None;
    }
    let arr = r.result.get("instances")?.as_array()?;
    let live = arr.iter().find(|i| {
        !i.get("is_cdo")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    })?;
    parse_class_instance(live)
}

/// Read a `*UObject` component pointer from `parent_addr` at
/// `offset` and return the resolved subcomponent address.
/// Returns `None` if the pointer is null or read fails.
///
/// Convenience wrapper for the universal "follow the player ->
/// HealthComponent" / "actor -> CharMovementComponent" pattern.
/// Equivalent to `read_u64`, plus null-check + zero-as-None
/// ergonomics.
pub fn read_component_ptr<S: DeserializeOwned>(
    api: &Api<S>,
    parent_addr: u64,
    offset: u64,
) -> Option<u64> {
    let v = read_u64(api, parent_addr, offset);
    if v == 0 { None } else { Some(v) }
}

fn walk_class_inner<S: DeserializeOwned>(
    api: &Api<S>,
    class: &str,
    max: usize,
    include_cdo: bool,
) -> Vec<ClassInstance> {
    let r = api.op(
        "walk_class",
        json!({"class": class, "max": max, "include_cdo": include_cdo}),
    );
    if !r.ok {
        return Vec::new();
    }
    let Some(arr) = r.result.get("instances").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter().filter_map(parse_class_instance).collect()
}

fn parse_class_instance(inst: &serde_json::Value) -> Option<ClassInstance> {
    let sel = inst.get("addr_selector")?.as_str()?.to_string();
    let addr_str = inst.get("addr")?.as_str()?;
    let addr = u64::from_str_radix(addr_str.trim_start_matches("0x"), 16).ok()?;
    let name = inst.get("name")?.as_str()?.to_string();
    let full_name = inst
        .get("full_name")
        .and_then(|v| v.as_str())
        .unwrap_or(&name)
        .to_string();
    Some(ClassInstance {
        addr_selector: sel,
        addr,
        name,
        full_name,
    })
}

// ---- Field-byte read helpers (typed) -----------------------------

/// Read N bytes from a UObject at `offset` and return them. Empty
/// `Vec` on error.
pub fn read_bytes<S: DeserializeOwned>(
    api: &Api<S>,
    addr: u64,
    offset: u64,
    length: u64,
) -> Vec<u8> {
    let r = api.op(
        "read_bytes",
        json!({
            "instance_selector": format!("addr:0x{addr:X}"),
            "offset": offset,
            "length": length,
        }),
    );
    if !r.ok {
        return Vec::new();
    }
    r.result
        .get("bytes_hex")
        .and_then(|v| v.as_str())
        .and_then(|s| hex::decode(s).ok())
        .unwrap_or_default()
}

/// Read an `i32` at `offset` from `addr`. Returns 0 on failure.
pub fn read_i32<S: DeserializeOwned>(api: &Api<S>, addr: u64, offset: u64) -> i32 {
    let b = read_bytes(api, addr, offset, 4);
    if b.len() < 4 {
        return 0;
    }
    i32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// Read a `u32` at `offset`. Returns 0 on failure.
pub fn read_u32<S: DeserializeOwned>(api: &Api<S>, addr: u64, offset: u64) -> u32 {
    let b = read_bytes(api, addr, offset, 4);
    if b.len() < 4 {
        return 0;
    }
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// Read an `f32` at `offset`. Returns 0.0 on failure.
pub fn read_f32<S: DeserializeOwned>(api: &Api<S>, addr: u64, offset: u64) -> f32 {
    let b = read_bytes(api, addr, offset, 4);
    if b.len() < 4 {
        return 0.0;
    }
    f32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// Read an `f64` at `offset`. Returns 0.0 on failure.
pub fn read_f64<S: DeserializeOwned>(api: &Api<S>, addr: u64, offset: u64) -> f64 {
    let b = read_bytes(api, addr, offset, 8);
    if b.len() < 8 {
        return 0.0;
    }
    f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

/// Read a `u8` at `offset`. Returns 0 on failure.
pub fn read_u8<S: DeserializeOwned>(api: &Api<S>, addr: u64, offset: u64) -> u8 {
    let b = read_bytes(api, addr, offset, 1);
    b.first().copied().unwrap_or(0)
}

/// Read a u64 (e.g. a UObject pointer) at `offset`. Returns 0 on failure.
pub fn read_u64<S: DeserializeOwned>(api: &Api<S>, addr: u64, offset: u64) -> u64 {
    let b = read_bytes(api, addr, offset, 8);
    if b.len() < 8 {
        return 0;
    }
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

// ----------------------------------------------------------------
// Byte-slice decoders: decode typed fields from a pre-fetched
// buffer without issuing another API call.
// ----------------------------------------------------------------

pub fn from_le_i32(buf: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}

pub fn from_le_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}

pub fn from_le_u64(buf: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(buf[off..off + 8].try_into().unwrap())
}

pub fn from_le_f32(buf: &[u8], off: usize) -> f32 {
    f32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}

pub fn from_le_f64(buf: &[u8], off: usize) -> f64 {
    f64::from_le_bytes(buf[off..off + 8].try_into().unwrap())
}

// ----------------------------------------------------------------
// TArray header reading.
// ----------------------------------------------------------------

pub struct TArrayHeader {
    pub ptr: u64,
    pub num: i32,
    pub max: i32,
}

/// Read a UE TArray header (ptr, num, max) at `offset` from `addr`.
/// Returns `None` if the read fails or returns too few bytes.
pub fn read_tarray_header<S: DeserializeOwned>(
    api: &Api<S>,
    addr: u64,
    offset: u64,
) -> Option<TArrayHeader> {
    let b = read_bytes(api, addr, offset, 16);
    if b.len() < 16 {
        return None;
    }
    Some(TArrayHeader {
        ptr: from_le_u64(&b, 0),
        num: from_le_i32(&b, 8),
        max: from_le_i32(&b, 12),
    })
}

// ----------------------------------------------------------------
// FName helpers.
// ----------------------------------------------------------------

/// Resolve an FName from its two u32 components (comparison_index
/// and number) by packing them into the u64 format and calling
/// `fname_to_string`.
pub fn fname_from_parts<S: DeserializeOwned>(
    api: &Api<S>,
    comparison_index: u32,
    number: u32,
) -> Option<String> {
    let packed: u64 = (comparison_index as u64) | ((number as u64) << 32);
    fname_to_string(api, packed)
}

// ----------------------------------------------------------------
// `sample_thread_modules` op response. Typed view + Display.
// ----------------------------------------------------------------

/// One module row in the per-process sample.
#[derive(Debug, Clone)]
pub struct ModuleSamples {
    pub module: String,
    pub samples: u64,
    pub pct: f64,
}

/// Per-thread breakdown.
#[derive(Debug, Clone)]
pub struct ThreadSampleRow {
    pub name: String,
    pub tid: u64,
    pub samples: u64,
    pub by_module: Vec<ModuleSamples>,
}

/// Decoded `sample_thread_modules` op response.
pub struct ThreadModulesReport {
    pub total_samples: u64,
    pub by_module_grand_total: Vec<ModuleSamples>,
    pub by_thread: Vec<ThreadSampleRow>,
}

impl ThreadModulesReport {
    /// Decode from an op response value (the `result` field).
    pub fn from_value(v: &serde_json::Value) -> Self {
        let total_samples = v.get("total_samples").and_then(|x| x.as_u64()).unwrap_or(0);
        let by_module_grand_total = v
            .get("by_module_grand_total")
            .and_then(|x| x.as_array())
            .map(|arr| arr.iter().filter_map(parse_module).collect())
            .unwrap_or_default();
        let by_thread = v
            .get("by_thread")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        Some(ThreadSampleRow {
                            name: t.get("name").and_then(|v| v.as_str())?.to_string(),
                            tid: t.get("tid").and_then(|v| v.as_u64()).unwrap_or(0),
                            samples: t.get("samples").and_then(|v| v.as_u64()).unwrap_or(0),
                            by_module: t
                                .get("by_module")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.iter().filter_map(parse_module).collect())
                                .unwrap_or_default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            total_samples,
            by_module_grand_total,
            by_thread,
        }
    }
}

fn parse_module(v: &serde_json::Value) -> Option<ModuleSamples> {
    Some(ModuleSamples {
        module: v.get("module").and_then(|x| x.as_str())?.to_string(),
        samples: v.get("samples").and_then(|x| x.as_u64()).unwrap_or(0),
        pct: v.get("pct").and_then(|x| x.as_f64()).unwrap_or(0.0),
    })
}

impl std::fmt::Display for ThreadModulesReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\ntotal samples: {}", self.total_samples)?;
        writeln!(f, "\n=== Grand total: which module is the process IN ===")?;
        writeln!(f, "{:>40} {:>10} {:>8}", "module", "samples", "%")?;
        writeln!(f, "{}", "-".repeat(62))?;
        for m in &self.by_module_grand_total {
            writeln!(f, "{:>40} {:>10} {:>7.2}%", m.module, m.samples, m.pct)?;
        }
        writeln!(f, "\n=== Per-thread breakdown ===")?;
        for t in self.by_thread.iter().take(20) {
            writeln!(f, "\n[{}] tid={} samples={}", t.name, t.tid, t.samples)?;
            for m in &t.by_module {
                writeln!(f, "  {:>40} {:>8} {:>6.2}%", m.module, m.samples, m.pct)?;
            }
        }
        Ok(())
    }
}

/// Run the `sample_thread_modules` op + decode the response in
/// one call. Pretty-print via `{}` Display.
pub fn sample_thread_modules<S: DeserializeOwned>(
    api: &Api<S>,
    duration_ms: u64,
    interval_ms: u64,
) -> ThreadModulesReport {
    let r = api.op(
        "sample_thread_modules",
        json!({"duration_ms": duration_ms, "interval_ms": interval_ms}),
    );
    if !r.ok {
        panic!("sample_thread_modules failed: {:?}", r.error);
    }
    ThreadModulesReport::from_value(&r.result)
}
