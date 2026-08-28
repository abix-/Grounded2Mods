//! Data-table catalog framework. One of ueforge's opinionated UE5
//! mod modules (rpg / stacks / difficulty / inventory / damage /
//! data_table).
//!
//! ```text
//! K8s slot: Def=DataTableDef, Registry=DataTableRegistry,
//!           Instance=live UDataTable + its RowMap rows,
//!           Controller=DataTableSnapshot::capture +
//!                      probe::discover_data_tables
//! ```
//!
//! Stacks + difficulty + status effects are all special cases of
//! "typed-field operations on iterated objects". The tables
//! themselves (`UDataTable` + row schema) are their own first-class
//! concept: a stable id + the UE short name of the data-table + the
//! row-struct schema (field name -> offset -> element size).
//!
//! Phase 1 (this module) is a READ-only browser surface: declare
//! known tables statically, snapshot rows on demand, render in
//! ImGui. Writes are deferred to a later phase that unifies stack +
//! difficulty + future field tweaks behind a single TweakDef.
//!
//! ```ignore
//! use ueforge::data_table::{DataTableDef, DataTableRegistry, RowField, RowSchema};
//!
//! static DT_MATERIALS: DataTableDef = DataTableDef {
//!     id: "materials",
//!     table_name: "DT_Materials",
//!     row_struct: RowSchema {
//!         name: "FMaterialRow",
//!         fields: &[
//!             RowField { name: "MaxCanStack", offset: 0x48, element_size: 4 },
//!         ],
//!     },
//! };
//!
//! pub static DATA_TABLES: DataTableRegistry =
//!     DataTableRegistry::new(&[&DT_MATERIALS]);
//! ```

/// One field on a data-table row struct: the property's name, its
/// byte offset within a row, and the size of one element. Mirrors
/// [`crate::ue::NativeProperty`] (the runtime form for UClasses);
/// kept as its own type because data-table schemas are typically
/// hand-declared static catalog data, while `NativeProperty` is
/// produced by walking the live FField chain.
#[derive(Debug, Clone, Copy)]
pub struct RowField {
    pub name: &'static str,
    pub offset: usize,
    pub element_size: u32,
}

/// Schema of a data-table row struct: the UE `UScriptStruct` short
/// name + the field list. Static-catalog form; the discovery path
/// (`probe::discover_data_tables`) emits the same shape as JSON.
#[derive(Debug, Clone, Copy)]
pub struct RowSchema {
    pub name: &'static str,
    pub fields: &'static [RowField],
}

/// One data-table the mod knows about: a stable id, the UE short
/// name to resolve via `datatable::find_by_short_name`, and the row
/// schema. Pure data; no runtime state.
pub struct DataTableDef {
    /// Stable id for [`DataTableRegistry::def`] lookup. Lowercase
    /// snake_case by convention (`"materials"`, `"status_effects"`).
    pub id: &'static str,
    /// UE short name of the `UDataTable` instance, resolved at
    /// snapshot time via
    /// [`crate::ue::datatable::find_by_short_name`].
    pub table_name: &'static str,
    /// Row-struct schema. Field offsets are decoded from the SDK
    /// headers; cross-checked at runtime against discovery in
    /// Phase 1e.
    pub row_struct: RowSchema,
}

/// Workspace-standard `<Subject>Registry` for data tables. Stores
/// `&[&'static DataTableDef]` (slice of refs) so consumers can
/// declare each Def as its own named static + inline the registry
/// literal. Mirrors `StackRegistry` / `StatusRegistry`.
pub struct DataTableRegistry {
    entries: &'static [&'static DataTableDef],
}

impl DataTableRegistry {
    pub const fn new(entries: &'static [&'static DataTableDef]) -> Self {
        Self { entries }
    }

    /// Canonical lookup. O(N) linear scan; registries are tiny.
    pub fn def(&self, id: &str) -> Option<&'static DataTableDef> {
        for d in self.entries {
            if d.id == id {
                return Some(*d);
            }
        }
        None
    }

    pub fn entries(&self) -> &'static [&'static DataTableDef] {
        self.entries
    }

    pub fn iter(&self) -> std::slice::Iter<'_, &'static DataTableDef> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<'a> IntoIterator for &'a DataTableRegistry {
    type Item = &'static DataTableDef;
    type IntoIter = std::iter::Copied<std::slice::Iter<'a, &'static DataTableDef>>;
    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter().copied()
    }
}

// ---- Process-global registration ----------------------------------------
//
// Game crates call `register(&MY_REGISTRY)` from their worker init
// so the `list_data_tables` debug op + ImGui surfaces can enumerate
// statically-declared tables. Optional: most consumers today rely
// on the runtime `discovery` cache, which covers every live table
// regardless of registration.

static REGISTERED: std::sync::OnceLock<&'static DataTableRegistry> = std::sync::OnceLock::new();

/// Register the process-wide `DataTableRegistry`. Idempotent on
/// first set; later calls are silently ignored (first writer wins,
/// same shape as the other framework registries). Game crates
/// call this from their `on_unreal_init` worker.
pub fn register(reg: &'static DataTableRegistry) {
    let _ = REGISTERED.set(reg);
}

/// Borrow the registered `DataTableRegistry`, if any consumer has
/// called [`register`]. None when no game crate has declared a
/// catalog yet; in that case use the runtime `discovery` cache
/// for table enumeration.
pub fn registered() -> Option<&'static DataTableRegistry> {
    REGISTERED.get().copied()
}

/// JSON of the registered `DataTableRegistry` (id + table_name +
/// row_struct schema). Powers the `list_data_tables` debug op +
/// the registry-only ImGui surface. Returns
/// `{"registered": false, ...}` when no consumer has registered.
pub fn list_json() -> serde_json::Value {
    use serde_json::json;
    let Some(reg) = registered() else {
        return json!({
            "registered": false,
            "count": 0,
            "tables": [],
            "note": "no DataTableRegistry registered. \
                     game crates call ueforge::data_table::register \
                     from their worker init to populate this.",
        });
    };
    let tables: Vec<serde_json::Value> = reg
        .iter()
        .map(|d| {
            let fields: Vec<serde_json::Value> = d
                .row_struct
                .fields
                .iter()
                .map(|f| {
                    json!({
                        "name": f.name,
                        "offset": f.offset,
                        "element_size": f.element_size,
                    })
                })
                .collect();
            json!({
                "id": d.id,
                "table_name": d.table_name,
                "row_struct": {
                    "name": d.row_struct.name,
                    "fields": fields,
                },
            })
        })
        .collect();
    json!({
        "registered": true,
        "count": tables.len(),
        "tables": tables,
    })
}

// ---- Snapshot (Phase 1b) -------------------------------------------------

use std::sync::OnceLock;

use serde_json::{Value as Json, json};

use crate::ue::{self, UObject};

// ---- Name-based tweaks ---------------------------------------------------

/// Look up `field_name` on the row struct of `table_name` in the
/// in-memory discovery cache. Returns `(offset, element_size, FProperty class)`.
///
/// Cheap (HashMap-like linear scan over the cached table list).
/// `None` if the table isn't in the cache or the field isn't on
/// its row struct. Callers should re-call after
/// `discovery::refresh()` if content streamed in late.
pub fn resolve_field(table_name: &str, field_name: &str) -> Option<(usize, u32, String)> {
    let snap = crate::discovery::cached()?;
    let tables = snap.data_tables.get("data_tables")?.as_array()?;
    for t in tables {
        if t.get("table_name").and_then(|v| v.as_str()) != Some(table_name) {
            continue;
        }
        let fields = t
            .get("row_struct")
            .and_then(|v| v.get("fields"))
            .and_then(|v| v.as_array())?;
        for f in fields {
            if f.get("name").and_then(|v| v.as_str()) != Some(field_name) {
                continue;
            }
            let offset = f.get("offset").and_then(|v| v.as_i64())? as usize;
            let size = f.get("element_size").and_then(|v| v.as_i64())? as u32;
            let class = f
                .get("class")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            return Some((offset, size, class));
        }
    }
    None
}

/// Name-based wrapper over `FieldTweak<T>`. The offset is
/// resolved from the discovery cache on first apply rather than
/// baked at declaration. This is the canonical write entry for
/// new-game bootstrap: a mod declares one static per knob, no
/// SDK header dive, no hand-typed offsets, and a game patch that
/// shifts the offset is picked up by the next discovery refresh.
///
/// ```ignore
/// use ueforge::data_table::NamedFieldTweak;
///
/// static MAX_STACK: NamedFieldTweak<i32> =
///     NamedFieldTweak::new("DT_Materials", "MaxCanStack");
///
/// // From `on_unreal_init` (after discovery::run_at_load):
/// MAX_STACK.apply(|v| v.saturating_mul(4), |v| v <= 1).ok();
/// ```
///
/// `T` semantics match `FieldTweak<T>`: vanilla snapshot captured
/// per row on first read; subsequent applies re-base on the captured
/// baseline, never compounding. Reverting via [`revert`](Self::revert)
/// writes vanilla back to every captured row.
pub struct NamedFieldTweak<T: Copy + PartialEq + Send + 'static> {
    table_name: &'static str,
    field_name: &'static str,
    inner: OnceLock<ue::datatable::FieldTweak<T>>,
}

impl<T: Copy + PartialEq + Send + 'static> NamedFieldTweak<T> {
    pub const fn new(table_name: &'static str, field_name: &'static str) -> Self {
        Self {
            table_name,
            field_name,
            inner: OnceLock::new(),
        }
    }

    pub fn table_name(&self) -> &'static str {
        self.table_name
    }

    pub fn field_name(&self) -> &'static str {
        self.field_name
    }

    /// Resolve the offset (lazily, once) and apply. Errors if the
    /// field isn't in the discovery cache. Caller can
    /// `discovery::refresh()` and try again.
    pub fn apply<F, S>(&self, transform: F, skip_if: S) -> Result<usize, String>
    where
        F: Fn(T) -> T,
        S: Fn(T) -> bool,
    {
        let tweak = self.resolve_or_init()?;
        tweak.apply(transform, skip_if)
    }

    /// Revert every captured row to its vanilla value.
    pub fn revert(&self) -> Result<usize, String> {
        let tweak = self.resolve_or_init()?;
        tweak.revert()
    }

    fn resolve_or_init(&self) -> Result<&ue::datatable::FieldTweak<T>, String> {
        if let Some(t) = self.inner.get() {
            return Ok(t);
        }
        let (offset, _size, _class) =
            resolve_field(self.table_name, self.field_name).ok_or_else(|| {
                format!(
                    "NamedFieldTweak: field '{}.{}' not in discovery cache",
                    self.table_name, self.field_name
                )
            })?;
        let tweak = ue::datatable::FieldTweak::new(self.table_name, offset);
        // First setter wins; second loses (we still got our copy back).
        let _ = self.inner.set(tweak);
        Ok(self.inner.get().expect("just initialized"))
    }

    /// Spawn a background worker that waits for the table to
    /// load (via `ue::datatable::on_first_sight`), refreshes the
    /// discovery cache so the field is resolvable, then applies
    /// once. Same shape as `FieldTweak::apply_when_ready`.
    /// the typical init-time entry point.
    pub fn apply_when_ready<F, S>(
        &'static self,
        timeout: std::time::Duration,
        transform: F,
        skip_if: S,
    ) where
        F: Fn(T) -> T + Send + 'static,
        S: Fn(T) -> bool + Send + 'static,
    {
        ue::datatable::on_first_sight(self.table_name, timeout, move |_dt| {
            // Re-walk discovery if our field isn't cached yet.
            // the table only just appeared.
            if resolve_field(self.table_name, self.field_name).is_none() {
                let _ = crate::discovery::refresh();
            }
            match self.apply(&transform, &skip_if) {
                Ok(n) => crate::log::log(format_args!(
                    "NamedFieldTweak<{}>: applied to {n} rows of {}.{}",
                    std::any::type_name::<T>(),
                    self.table_name,
                    self.field_name
                )),
                Err(e) => crate::log::log(format_args!(
                    "NamedFieldTweak<{}>: apply failed on {}.{}: {e}",
                    std::any::type_name::<T>(),
                    self.table_name,
                    self.field_name
                )),
            }
        });
    }
}

// ---- Dynamic (runtime-named) tweaks --------------------------------------
//
// Settings-driven tweaks: at load, a mod reads a list of
// `(table, field, kind, value)` entries from disk and applies
// them. The (table, field) names aren't known at compile time,
// so NamedFieldTweak's `&'static str` requirement doesn't fit.
//
// Solution: a small global registry, one per primitive `T`, that
// leaks (table_name, field) on first sight and stores a
// `&'static FieldTweak<T>` keyed by `(table, field)`. Subsequent
// applies of the same tweak hit the same FieldTweak instance so
// the vanilla cache + idempotent re-apply semantics carry across
// settings reloads.

use std::collections::HashMap;

use parking_lot::Mutex as PlMutex;

type DynRegistry<T> = PlMutex<HashMap<(String, String), &'static ue::datatable::FieldTweak<T>>>;

static DYN_I32: OnceLock<DynRegistry<i32>> = OnceLock::new();
static DYN_F32: OnceLock<DynRegistry<f32>> = OnceLock::new();
static DYN_U32: OnceLock<DynRegistry<u32>> = OnceLock::new();

fn dyn_registry<T: Copy + PartialEq + Send + 'static>(
    cell: &'static OnceLock<DynRegistry<T>>,
) -> &'static DynRegistry<T> {
    cell.get_or_init(|| PlMutex::new(HashMap::new()))
}

fn dyn_tweak<T: Copy + PartialEq + Send + 'static>(
    reg: &'static DynRegistry<T>,
    table: &str,
    field: &str,
) -> Result<&'static ue::datatable::FieldTweak<T>, String> {
    let key = (table.to_string(), field.to_string());
    if let Some(t) = reg.lock().get(&key) {
        return Ok(*t);
    }
    let (offset, _size, _class) = resolve_field(table, field)
        .ok_or_else(|| format!("dynamic_apply: field '{table}.{field}' not in discovery cache"))?;
    // Leak the table name so FieldTweak's &'static str is satisfied
    // for the process lifetime. One leak per unique table name.
    let table_static: &'static str = Box::leak(table.to_string().into_boxed_str());
    let tw: &'static ue::datatable::FieldTweak<T> = Box::leak(Box::new(
        ue::datatable::FieldTweak::new(table_static, offset),
    ));
    reg.lock().insert(key, tw);
    Ok(tw)
}

/// Apply an i32 transform to `table.field` (names resolved from
/// the discovery cache). Vanilla per-row is captured on first
/// apply; subsequent applies re-base on the captured baseline.
///
/// `skip_if` runs against the vanilla value. Typical use is
/// `|v| v <= 1` to leave non-stackable rows alone.
pub fn dynamic_apply_i32(
    table: &str,
    field: &str,
    transform: impl Fn(i32) -> i32,
    skip_if: impl Fn(i32) -> bool,
) -> Result<usize, String> {
    let reg = dyn_registry::<i32>(&DYN_I32);
    let tw = dyn_tweak::<i32>(reg, table, field)?;
    tw.apply(transform, skip_if)
}

/// Apply an f32 transform to `table.field`.
pub fn dynamic_apply_f32(
    table: &str,
    field: &str,
    transform: impl Fn(f32) -> f32,
    skip_if: impl Fn(f32) -> bool,
) -> Result<usize, String> {
    let reg = dyn_registry::<f32>(&DYN_F32);
    let tw = dyn_tweak::<f32>(reg, table, field)?;
    tw.apply(transform, skip_if)
}

/// Apply a u32 transform to `table.field`.
pub fn dynamic_apply_u32(
    table: &str,
    field: &str,
    transform: impl Fn(u32) -> u32,
    skip_if: impl Fn(u32) -> bool,
) -> Result<usize, String> {
    let reg = dyn_registry::<u32>(&DYN_U32);
    let tw = dyn_tweak::<u32>(reg, table, field)?;
    tw.apply(transform, skip_if)
}

/// Revert every dynamic tweak captured so far (all primitive T's).
/// Returns total touched-row count across the three registries.
pub fn dynamic_revert_all() -> usize {
    let mut total = 0usize;
    if let Some(reg) = DYN_I32.get() {
        for (_k, tw) in reg.lock().iter() {
            if let Ok(n) = tw.revert() {
                total += n;
            }
        }
    }
    if let Some(reg) = DYN_F32.get() {
        for (_k, tw) in reg.lock().iter() {
            if let Ok(n) = tw.revert() {
                total += n;
            }
        }
    }
    if let Some(reg) = DYN_U32.get() {
        for (_k, tw) in reg.lock().iter() {
            if let Ok(n) = tw.revert() {
                total += n;
            }
        }
    }
    total
}

/// Revert one specific `(table, field)` dynamic tweak across every
/// primitive registry it might have been registered in. Returns the
/// total rows reverted. Zero when nothing matches.
pub fn dynamic_revert_one(table: &str, field: &str) -> usize {
    let key = (table.to_string(), field.to_string());
    let mut total = 0usize;
    if let Some(reg) = DYN_I32.get() {
        if let Some(tw) = reg.lock().get(&key)
            && let Ok(n) = tw.revert()
        {
            total += n;
        }
    }
    if let Some(reg) = DYN_F32.get() {
        if let Some(tw) = reg.lock().get(&key)
            && let Ok(n) = tw.revert()
        {
            total += n;
        }
    }
    if let Some(reg) = DYN_U32.get() {
        if let Some(tw) = reg.lock().get(&key)
            && let Ok(n) = tw.revert()
        {
            total += n;
        }
    }
    total
}

/// How many vanilla baselines have been captured for the data-
/// table dynamic tweak at `(table, field)`. Walks each primitive
/// registry; returns the first hit. Used by surface code that
/// reports per-tweak status.
pub fn dynamic_vanilla_count(table: &str, field: &str) -> usize {
    let key = (table.to_string(), field.to_string());
    if let Some(reg) = DYN_I32.get()
        && let Some(tw) = reg.lock().get(&key)
    {
        return tw.vanilla_count();
    }
    if let Some(reg) = DYN_F32.get()
        && let Some(tw) = reg.lock().get(&key)
    {
        return tw.vanilla_count();
    }
    if let Some(reg) = DYN_U32.get()
        && let Some(tw) = reg.lock().get(&key)
    {
        return tw.vanilla_count();
    }
    0
}

/// JSON list of every active dynamic tweak across the three
/// primitive registries. Each entry: `{kind: "i32"|"f32"|"u32",
/// table_name, offset, vanilla_count}`. Powers the
/// `tweak_list` debug op.
pub fn dynamic_list_json() -> Json {
    let mut tweaks: Vec<Json> = Vec::new();
    if let Some(reg) = DYN_I32.get() {
        for ((table, field), tw) in reg.lock().iter() {
            tweaks.push(json!({
                "kind": "i32",
                "table_name": table,
                "field_name": field,
                "offset": tw.offset(),
                "vanilla_count": tw.vanilla_count(),
            }));
        }
    }
    if let Some(reg) = DYN_F32.get() {
        for ((table, field), tw) in reg.lock().iter() {
            tweaks.push(json!({
                "kind": "f32",
                "table_name": table,
                "field_name": field,
                "offset": tw.offset(),
                "vanilla_count": tw.vanilla_count(),
            }));
        }
    }
    if let Some(reg) = DYN_U32.get() {
        for ((table, field), tw) in reg.lock().iter() {
            tweaks.push(json!({
                "kind": "u32",
                "table_name": table,
                "field_name": field,
                "offset": tw.offset(),
                "vanilla_count": tw.vanilla_count(),
            }));
        }
    }
    json!({
        "count": tweaks.len(),
        "tweaks": tweaks,
    })
}

// ---- Persistence (Phase 2) ----------------------------------------------
//
// Applied tweaks live in process memory only by default; a Ctrl+R
// hot-reload or a relaunch wipes them. The persistence layer here
// writes every successful `tweak_apply` to `<DLL_dir>/tweaks.json`
// and reloads on init so the in-flight tweaks restore themselves
// across reloads.
//
// File shape (schema_version 1):
// ```json
// {
//   "schema_version": 1,
//   "tweaks": [
//     {"table": "DT_Materials", "field": "MaxCanStack",
//      "kind": "i32", "op": "multiply", "value": 4},
//     ...
//   ]
// }
// ```

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersistedTweak {
    pub table: String,
    pub field: String,
    pub kind: String,
    pub op: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TweaksFile {
    #[serde(default = "default_tweaks_schema_version")]
    schema_version: u32,
    #[serde(default)]
    tweaks: Vec<PersistedTweak>,
}

fn default_tweaks_schema_version() -> u32 {
    1
}

impl Default for TweaksFile {
    fn default() -> Self {
        Self {
            schema_version: 1,
            tweaks: Vec::new(),
        }
    }
}

/// Process-memory mirror of `<DLL_dir>/tweaks.json`. Updated
/// in-band with every successful `tweak_apply` / `tweak_revert`
/// so the on-disk file is the source of truth for reload.
static PERSISTED: OnceLock<PlMutex<Vec<PersistedTweak>>> = OnceLock::new();

fn persisted_mutex() -> &'static PlMutex<Vec<PersistedTweak>> {
    PERSISTED.get_or_init(|| PlMutex::new(Vec::new()))
}

fn tweaks_file_path() -> std::path::PathBuf {
    let mut p = crate::log::dll_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    p.push("tweaks.json");
    p
}

/// JSON of the in-memory persisted-tweak list (mirrors the
/// `<DLL_dir>/tweaks.json` file). Powers `tweak_persisted_list`.
pub fn persisted_list_json() -> Json {
    let v = persisted_mutex().lock();
    let entries: Vec<Json> = v
        .iter()
        .map(|t| {
            json!({
                "table": t.table,
                "field": t.field,
                "kind": t.kind,
                "op": t.op,
                "value": t.value,
            })
        })
        .collect();
    json!({
        "count": entries.len(),
        "tweaks": entries,
        "path": tweaks_file_path().display().to_string(),
    })
}

/// Read `<DLL_dir>/tweaks.json` into the in-memory mirror. Missing
/// or unparseable files clear the mirror and log; not an error.
/// Idempotent.
pub fn load_persisted_from_disk() -> Json {
    use std::fs;
    let path = tweaks_file_path();
    let file: TweaksFile = match fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(f) => f,
            Err(e) => {
                crate::log::log(format_args!(
                    "tweaks: parse failed for {} ({e}); starting fresh",
                    path.display()
                ));
                TweaksFile::default()
            }
        },
        Err(_) => {
            crate::log::log(format_args!(
                "tweaks: no prior file at {}; starting fresh",
                path.display()
            ));
            TweaksFile::default()
        }
    };

    let count = file.tweaks.len();
    *persisted_mutex().lock() = file.tweaks;
    json!({
        "path": path.display().to_string(),
        "loaded": count,
        "schema_version": file.schema_version,
    })
}

/// Atomically write the in-memory mirror to `<DLL_dir>/tweaks.json`.
/// Errors return a string; callers MUST surface or log.
fn save_persisted_to_disk() -> Result<(), String> {
    use std::fs;
    use std::io::Write;
    let path = tweaks_file_path();
    let file = TweaksFile {
        schema_version: 1,
        tweaks: persisted_mutex().lock().clone(),
    };
    let json = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("tweaks: serialize failed: {e}"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("tweaks: mkdir {} failed: {e}", parent.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp)
            .map_err(|e| format!("tweaks: create {} failed: {e}", tmp.display()))?;
        f.write_all(json.as_bytes())
            .map_err(|e| format!("tweaks: write {} failed: {e}", tmp.display()))?;
        f.sync_all()
            .map_err(|e| format!("tweaks: fsync {} failed: {e}", tmp.display()))?;
    }
    fs::rename(&tmp, &path).map_err(|e| {
        format!(
            "tweaks: rename {} -> {} failed: {e}",
            tmp.display(),
            path.display()
        )
    })?;
    Ok(())
}

/// Upsert a persisted-tweak record (replace by (table, field)
/// key; append if absent), then save to disk. Called from
/// `tweak_apply_from_args` after a successful in-memory apply.
fn record_persisted(t: PersistedTweak) -> Result<(), String> {
    {
        let mut v = persisted_mutex().lock();
        if let Some(pos) = v
            .iter()
            .position(|p| p.table == t.table && p.field == t.field)
        {
            v[pos] = t;
        } else {
            v.push(t);
        }
    }
    save_persisted_to_disk()
}

/// Remove a persisted-tweak record by `(table, field)`. Saves on
/// success. Returns `true` if an entry was removed.
fn forget_persisted(table: &str, field: &str) -> Result<bool, String> {
    let removed = {
        let mut v = persisted_mutex().lock();
        if let Some(pos) = v.iter().position(|p| p.table == table && p.field == field) {
            v.remove(pos);
            true
        } else {
            false
        }
    };
    if removed {
        save_persisted_to_disk()?;
    }
    Ok(removed)
}

/// Public wrapper around the private `forget_persisted` for the
/// `tweak_revert` op. Removes one `(table, field)` entry from
/// `<DLL_dir>/tweaks.json` and the in-memory mirror.
pub fn forget_persisted_pub(table: &str, field: &str) -> Result<bool, String> {
    forget_persisted(table, field)
}

/// Public wrapper around the private `forget_persisted_all` for
/// the `tweak_revert` op (no-args form). Clears every entry from
/// `<DLL_dir>/tweaks.json` and the in-memory mirror.
pub fn forget_persisted_all_pub() -> Result<usize, String> {
    forget_persisted_all()
}

fn forget_persisted_all() -> Result<usize, String> {
    let n = {
        let mut v = persisted_mutex().lock();
        let n = v.len();
        v.clear();
        n
    };
    if n > 0 {
        save_persisted_to_disk()?;
    }
    Ok(n)
}

/// Boot-time entry: read `<DLL_dir>/tweaks.json` into the
/// in-memory mirror, then re-apply every entry. Call from the
/// mod's `on_unreal_init` worker AFTER `discovery::run_at_load`
/// so field offsets resolve. Returns a JSON `{loaded, applied,
/// failed}` report; safe to call when the file is missing
/// (returns `applied: 0`, no error).
pub fn restore_persisted_at_init() -> Json {
    let load = load_persisted_from_disk();
    let apply = reapply_persisted();
    json!({
        "loaded": load.get("loaded").cloned().unwrap_or(json!(0)),
        "applied": apply.get("applied").cloned().unwrap_or(json!(0)),
        "failed": apply.get("failed").cloned().unwrap_or(json!([])),
        "path": load.get("path").cloned().unwrap_or(json!("")),
    })
}

/// Re-apply every persisted tweak. Call from the mod's
/// `on_unreal_init` worker AFTER `discovery::run_at_load` so the
/// field offsets can resolve. Returns a JSON report.
///
/// Each entry is applied via `tweak_apply_from_args` BUT without
/// re-persisting (would no-op since the entry is already in the
/// list, but skips the IO).
pub fn reapply_persisted() -> Json {
    let snapshot: Vec<PersistedTweak> = persisted_mutex().lock().clone();
    let mut applied = 0usize;
    let mut failed: Vec<Json> = Vec::new();
    for t in &snapshot {
        let args = json!({
            "table": t.table,
            "field": t.field,
            "kind": t.kind,
            "op": t.op,
            "value": t.value,
        });
        match tweak_apply_inner(&args, /*persist=*/ false) {
            Ok(_) => applied += 1,
            Err(e) => failed.push(json!({
                "table": t.table,
                "field": t.field,
                "kind": t.kind,
                "op": t.op,
                "value": t.value,
                "error": e,
            })),
        }
    }
    crate::log::log(format_args!(
        "tweaks: reapplied {applied}/{} persisted tweaks; {} failed",
        snapshot.len(),
        failed.len()
    ));
    json!({
        "applied": applied,
        "failed": failed,
    })
}

/// Op-dispatch entry. Parses the JSON args of the `tweak_apply`
/// debug op and routes to the right `dynamic_apply_*` per `kind`.
///
/// Args shape:
/// ```json
/// {
///   "table": "DT_Materials",
///   "field": "MaxCanStack",
///   "kind":  "i32",        // i32 | f32 | u32
///   "op":    "multiply",   // set | multiply | add
///   "value": 4.0
/// }
/// ```
///
/// `set` writes the literal value. `multiply` multiplies the
/// captured vanilla by the value (vanilla * value). `add` adds.
/// Operations always re-base on the captured vanilla so they're
/// idempotent: repeated calls with the same op + value land the
/// same final field value.
pub fn tweak_apply_from_args(args: &Json) -> Result<Json, String> {
    tweak_apply_inner(args, /*persist=*/ true)
}

/// Inner form. `persist=false` skips writing to
/// `<DLL_dir>/tweaks.json`; used by [`reapply_persisted`] so
/// reload-time replay doesn't trigger N redundant IO.
fn tweak_apply_inner(args: &Json, persist: bool) -> Result<Json, String> {
    let table = args
        .get("table")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "tweak_apply: missing 'table'".to_string())?;
    let field = args
        .get("field")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "tweak_apply: missing 'field'".to_string())?;
    let kind = args
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "tweak_apply: missing 'kind' (i32|f32|u32)".to_string())?;
    let op = args
        .get("op")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "tweak_apply: missing 'op' (set|multiply|add)".to_string())?;
    let value = args
        .get("value")
        .ok_or_else(|| "tweak_apply: missing 'value'".to_string())?;

    let touched = match kind {
        "i32" => {
            let v = value
                .as_i64()
                .ok_or_else(|| format!("tweak_apply: 'value' not i64-like: {value}"))?
                as i32;
            match op {
                "set" => dynamic_apply_i32(table, field, |_| v, |_| false)?,
                "multiply" => dynamic_apply_i32(
                    table,
                    field,
                    |vanilla| vanilla.saturating_mul(v),
                    |vanilla| vanilla == 0,
                )?,
                "add" => {
                    dynamic_apply_i32(table, field, |vanilla| vanilla.saturating_add(v), |_| false)?
                }
                other => return Err(format!("tweak_apply: unknown i32 op '{other}'")),
            }
        }
        "u32" => {
            let v = value
                .as_u64()
                .ok_or_else(|| format!("tweak_apply: 'value' not u64-like: {value}"))?
                as u32;
            match op {
                "set" => dynamic_apply_u32(table, field, |_| v, |_| false)?,
                "multiply" => dynamic_apply_u32(
                    table,
                    field,
                    |vanilla| vanilla.saturating_mul(v),
                    |vanilla| vanilla == 0,
                )?,
                "add" => {
                    dynamic_apply_u32(table, field, |vanilla| vanilla.saturating_add(v), |_| false)?
                }
                other => return Err(format!("tweak_apply: unknown u32 op '{other}'")),
            }
        }
        "f32" => {
            let v = value
                .as_f64()
                .ok_or_else(|| format!("tweak_apply: 'value' not f64-like: {value}"))?
                as f32;
            match op {
                "set" => dynamic_apply_f32(table, field, |_| v, |_| false)?,
                "multiply" => dynamic_apply_f32(
                    table,
                    field,
                    move |vanilla| vanilla * v,
                    |vanilla| !vanilla.is_finite(),
                )?,
                "add" => dynamic_apply_f32(
                    table,
                    field,
                    move |vanilla| vanilla + v,
                    |vanilla| !vanilla.is_finite(),
                )?,
                other => return Err(format!("tweak_apply: unknown f32 op '{other}'")),
            }
        }
        other => return Err(format!("tweak_apply: unknown kind '{other}' (i32|f32|u32)")),
    };

    crate::log::log(format_args!(
        "tweak_apply: {kind} {op} {value} on '{table}.{field}' -> {touched} rows"
    ));

    if persist {
        let entry = PersistedTweak {
            table: table.to_string(),
            field: field.to_string(),
            kind: kind.to_string(),
            op: op.to_string(),
            value: value.clone(),
        };
        if let Err(e) = record_persisted(entry) {
            crate::log::log(format_args!(
                "tweak_apply: persistence write failed ({e}); in-memory \
                 apply succeeded but won't survive reload"
            ));
        }
    }

    Ok(json!({
        "table": table,
        "field": field,
        "kind": kind,
        "op": op,
        "value": value,
        "rows_touched": touched,
        "persisted": persist,
    }))
}

// ---- Class field, name-based --------------------------------------------

/// Look up `field_name` on `class_name` in the discovery cache's
/// classes JSON. Returns `(offset, element_size)`. `None` if the
/// class isn't in the cache or the field isn't on it.
pub fn resolve_class_field(class_name: &str, field_name: &str) -> Option<(usize, u32)> {
    let snap = crate::discovery::cached()?;
    let classes = snap.classes.get("classes")?.as_array()?;
    for c in classes {
        if c.get("name").and_then(|v| v.as_str()) != Some(class_name) {
            continue;
        }
        let fields = c.get("fields")?.as_array()?;
        for f in fields {
            if f.get("name").and_then(|v| v.as_str()) != Some(field_name) {
                continue;
            }
            let offset = f.get("offset").and_then(|v| v.as_i64())? as usize;
            let size = f.get("element_size").and_then(|v| v.as_i64())? as u32;
            return Some((offset, size));
        }
    }
    None
}

/// Name-based wrapper over [`ue::class_tweak::ClassFieldTweak<T>`].
/// Same shape as [`NamedFieldTweak<T>`] but for live UObjects: a
/// mod declares one static per knob with `(class_name, field_name)`,
/// the offset is resolved from the discovery cache on first apply,
/// and the underlying `ClassFieldTweak<T>` provides per-instance
/// vanilla snapshot + idempotent re-apply.
///
/// ```ignore
/// static SURVIVAL_HUNGER: ClassNamedFieldTweak<f32> =
///     ClassNamedFieldTweak::new("SurvivalComponent", "HungerDecayRate");
///
/// // From the mod's on_unreal_init worker (post discovery):
/// SURVIVAL_HUNGER.apply(|_obj| true, |v| Some(v * 0.5)).ok();
/// ```
pub struct ClassNamedFieldTweak<T: Copy + PartialEq + Send + 'static> {
    class_name: &'static str,
    field_name: &'static str,
    inner: OnceLock<ue::class_tweak::ClassFieldTweak<T>>,
}

impl<T: Copy + PartialEq + Send + 'static> ClassNamedFieldTweak<T> {
    pub const fn new(class_name: &'static str, field_name: &'static str) -> Self {
        Self {
            class_name,
            field_name,
            inner: OnceLock::new(),
        }
    }

    pub fn class_name(&self) -> &'static str {
        self.class_name
    }

    pub fn field_name(&self) -> &'static str {
        self.field_name
    }

    pub fn apply<Filter, Transform>(
        &self,
        filter: Filter,
        transform: Transform,
    ) -> Result<ue::class_tweak::ClassTweakStats, String>
    where
        Filter: Fn(&UObject) -> bool,
        Transform: Fn(T) -> Option<T>,
    {
        let tweak = self.resolve_or_init()?;
        tweak.apply(filter, transform)
    }

    pub fn revert(&self) -> Result<usize, String> {
        let tweak = self.resolve_or_init()?;
        tweak.revert()
    }

    fn resolve_or_init(&self) -> Result<&ue::class_tweak::ClassFieldTweak<T>, String> {
        if let Some(t) = self.inner.get() {
            return Ok(t);
        }
        let (offset, _size) =
            resolve_class_field(self.class_name, self.field_name).ok_or_else(|| {
                format!(
                    "ClassNamedFieldTweak: field '{}.{}' not in discovery cache",
                    self.class_name, self.field_name
                )
            })?;
        let tweak = ue::class_tweak::ClassFieldTweak::new(self.class_name, offset);
        let _ = self.inner.set(tweak);
        Ok(self.inner.get().expect("just initialized"))
    }
}

/// Capture every row of `table_name`, decoding fields per the live
/// `FProperty` chain on the table's `RowStruct`. Up to `max_rows`
/// entries are returned (`None` = all). Cold path; allocates freely.
///
/// Decode coverage (FProperty class -> JSON):
/// - `IntProperty` (i32), `UInt32Property` (u32), `Int64Property` (i64),
///   `UInt64Property` (u64), `ByteProperty` (u8 number)
/// - `FloatProperty` (f32), `DoubleProperty` (f64)
/// - `BoolProperty` (single-byte interpretation; bitfield bools land
///   as numeric. They need their bitmask which isn't in the FField
///   chain we walk today)
/// - `NameProperty` (resolved FName -> string)
/// - `StrProperty` (FString -> string)
/// - `ObjectProperty` / `SoftObjectProperty` / `ClassProperty` /
///   `WeakObjectProperty`. Pointer hex
/// - Everything else -- `{ "raw_bytes_hex": "..." }` of the first
///   `element_size` bytes so a future decoder can be added without
///   re-walking.
///
/// Returns `None` if the table can't be resolved or has no RowStruct.
pub fn snapshot_table(table_name: &str, max_rows: Option<usize>) -> Option<Json> {
    let table = ue::datatable::find_by_short_name(table_name)?;
    // SAFETY: `table` is a live UDataTable; ROW_STRUCT is the
    // documented offset of its `RowStruct` UScriptStruct pointer.
    // read_unaligned because nothing guarantees pointer alignment
    // on the row layout.
    let row_struct_ptr: *const UObject = unsafe {
        (table
            .as_ptr()
            .add(crate::ue::offsets::datatable::ROW_STRUCT) as *const *const UObject)
            .read_unaligned()
    };
    // SAFETY: row_struct_ptr was just read from the live table;
    // .as_ref() returns None for null without dereferencing.
    let row_struct = unsafe { row_struct_ptr.as_ref()? };
    let schema = ue::probe::walk_struct_fields(row_struct);

    let rt = ue::try_runtime()?;
    let mut rows: Vec<Json> = Vec::new();
    let mut total_rows = 0usize;
    // SAFETY: iter_rows is documented as unsafe over the live
    // UDataTable + its row map; each yielded (fname, ptr) pair is
    // valid while we hold the &UObject. transmute of fname_key
    // u64 to FName matches the engine's 8-byte FName layout.
    unsafe {
        for (fname_key, row_ptr) in ue::datatable::iter_rows(table) {
            total_rows += 1;
            if let Some(limit) = max_rows {
                if rows.len() >= limit {
                    continue;
                }
            }
            let fname = crate::ue::FName::from_u64(fname_key);
            let row_name = rt.name_resolver.to_string(fname);
            let fields_json = decode_row_fields(row_ptr, &schema);
            rows.push(json!({
                "row_name": row_name,
                "row_fname": fname_key,
                "fields": fields_json,
            }));
        }
    }

    Some(json!({
        "table_name": table_name,
        "full_path": table.full_name(),
        "row_struct": {
            "name": row_struct.name(),
            "fields": schema,
        },
        "rows_total": total_rows,
        "rows_returned": rows.len(),
        "rows": rows,
    }))
}

// ---- Drift validation (Phase 1e) -----------------------------------------

/// Cross-check every `DataTableDef` in `reg` against the live
/// reflection data ueforge::discovery captured. Each declared
/// field is compared on `(offset, element_size)` against the
/// FProperty entry of the same name on the table's RowStruct.
///
/// Returns a JSON report:
///
/// ```text
/// { "tables": [
///     { "id": "materials", "status": "ok" },
///     { "id": "recipes",
///       "status": "drift",
///       "issues": [
///         {"field": "Yield", "kind": "offset",
///          "declared": 0x10, "live": 0x14}
///       ]
///     },
///     { "id": "obsolete",
///       "status": "table_missing" }
///   ],
///   "ok_count": ...,
///   "drift_count": ...,
///   "missing_count": ... }
/// ```
///
/// Cold path; run once at mod init after `discovery::run_at_load`.
/// Logs every drift line via `ueforge::log!` so missed UE patches
/// surface in the deployed log even if the JSON is never read.
pub fn validate_registry(reg: &DataTableRegistry) -> Json {
    let mut tables_out: Vec<Json> = Vec::new();
    let mut ok = 0usize;
    let mut drift = 0usize;
    let mut missing = 0usize;

    let discovery = crate::discovery::cached();
    let empty_arr = Vec::new();
    let live_tables = discovery
        .as_ref()
        .and_then(|d| d.data_tables.get("data_tables").and_then(|v| v.as_array()))
        .unwrap_or(&empty_arr);

    for def in reg.entries() {
        let live = live_tables.iter().find(|t| {
            t.get("table_name")
                .and_then(|v| v.as_str())
                .is_some_and(|n| n == def.table_name)
        });
        let Some(live) = live else {
            missing += 1;
            crate::log::log(format_args!(
                "DataTable drift: '{}' (id={}) not present in discovery cache",
                def.table_name, def.id
            ));
            tables_out.push(json!({
                "id": def.id,
                "table_name": def.table_name,
                "status": "table_missing",
            }));
            continue;
        };

        let live_fields = live
            .get("row_struct")
            .and_then(|v| v.get("fields"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut issues: Vec<Json> = Vec::new();
        for declared in def.row_struct.fields {
            let live_f = live_fields.iter().find(|f| {
                f.get("name")
                    .and_then(|v| v.as_str())
                    .is_some_and(|n| n == declared.name)
            });
            let Some(lf) = live_f else {
                issues.push(json!({
                    "field": declared.name,
                    "kind": "missing",
                    "declared_offset": declared.offset,
                }));
                continue;
            };
            let live_offset = lf.get("offset").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
            let live_size = lf.get("element_size").and_then(|v| v.as_i64()).unwrap_or(0) as u32;
            if live_offset != declared.offset {
                issues.push(json!({
                    "field": declared.name,
                    "kind": "offset",
                    "declared": declared.offset,
                    "live": live_offset,
                }));
            }
            if live_size != declared.element_size {
                issues.push(json!({
                    "field": declared.name,
                    "kind": "element_size",
                    "declared": declared.element_size,
                    "live": live_size,
                }));
            }
        }

        if issues.is_empty() {
            ok += 1;
            tables_out.push(json!({
                "id": def.id,
                "table_name": def.table_name,
                "status": "ok",
            }));
        } else {
            drift += 1;
            for iss in &issues {
                crate::log::log(format_args!(
                    "DataTable drift: '{}' field '{}' {}: declared={:?} live={:?}",
                    def.table_name,
                    iss.get("field").and_then(|v| v.as_str()).unwrap_or("?"),
                    iss.get("kind").and_then(|v| v.as_str()).unwrap_or("?"),
                    iss.get("declared"),
                    iss.get("live"),
                ));
            }
            tables_out.push(json!({
                "id": def.id,
                "table_name": def.table_name,
                "status": "drift",
                "issues": issues,
            }));
        }
    }

    json!({
        "tables": tables_out,
        "ok_count": ok,
        "drift_count": drift,
        "missing_count": missing,
    })
}

fn decode_row_fields(row_ptr: *const u8, schema: &[Json]) -> Json {
    let mut map = serde_json::Map::with_capacity(schema.len());
    for f in schema {
        let name = f["name"].as_str().unwrap_or("<no-name>").to_string();
        let class = f["class"].as_str().unwrap_or("");
        let offset = f["offset"].as_i64().unwrap_or(0) as usize;
        let element_size = f["element_size"].as_i64().unwrap_or(0) as usize;
        // SAFETY: row_ptr came from iter_rows (a live row in a
        // live UDataTable); offset / element_size / class came
        // from the discovery cache's reflected FProperty chain so
        // the byte read matches the engine's row layout.
        let value = unsafe { decode_field(row_ptr, offset, element_size, class) };
        map.insert(name, value);
    }
    Json::Object(map)
}

unsafe fn decode_field(
    row_ptr: *const u8,
    offset: usize,
    element_size: usize,
    class: &str,
) -> Json {
    // SAFETY: caller's `unsafe fn` contract requires row_ptr +
    // offset to be a valid byte address within a live row, and
    // `class` to be the FProperty class name reflected for that
    // field. Each match arm casts to the matching T and reads
    // via read_unaligned (or unaligned-safe slice construction
    // for FString/raw bytes). No allocation happens through a
    // raw pointer; `&*ptr` extension for Object* yields a UObject
    // ref whose lifetime is tied to GObjects.
    unsafe {
        let p = row_ptr.add(offset);
        match class {
            "IntProperty" | "Int32Property" => {
                json!((p as *const i32).read_unaligned())
            }
            "UInt32Property" => json!((p as *const u32).read_unaligned()),
            "Int64Property" => json!((p as *const i64).read_unaligned()),
            "UInt64Property" => json!((p as *const u64).read_unaligned()),
            "Int16Property" => json!((p as *const i16).read_unaligned()),
            "UInt16Property" => json!((p as *const u16).read_unaligned()),
            "Int8Property" | "SByteProperty" => json!((p as *const i8).read_unaligned()),
            "ByteProperty" => json!((p as *const u8).read()),
            "BoolProperty" => json!((p as *const u8).read() != 0),
            "FloatProperty" => json!((p as *const f32).read_unaligned()),
            "DoubleProperty" => json!((p as *const f64).read_unaligned()),
            "NameProperty" => {
                let fname: crate::ue::fname::FName =
                    (p as *const crate::ue::fname::FName).read_unaligned();
                if fname.is_none() {
                    return Json::Null;
                }
                let rt = match ue::try_runtime() {
                    Some(r) => r,
                    None => return Json::Null,
                };
                json!(rt.name_resolver.to_string(fname))
            }
            "StrProperty" => {
                // FString = TArray<u16> { Data, Num, Max }
                let data: *const u16 = (p as *const *const u16).read_unaligned();
                let num = (p.add(8) as *const i32).read_unaligned();
                if data.is_null() || num <= 0 {
                    return json!("");
                }
                // Strip trailing NUL if present (FString stores it).
                let len = if num > 0 { (num - 1) as usize } else { 0 };
                let slice = std::slice::from_raw_parts(data, len);
                json!(String::from_utf16_lossy(slice))
            }
            "ObjectProperty" | "ClassProperty" | "SoftObjectProperty" | "SoftClassProperty"
            | "WeakObjectProperty" | "LazyObjectProperty" => {
                let ptr: *const UObject = (p as *const *const UObject).read_unaligned();
                if ptr.is_null() {
                    return Json::Null;
                }
                let obj = &*ptr;
                json!({
                    "addr": format!("0x{:x}", ptr as usize),
                    "name": obj.name(),
                })
            }
            _ => {
                let len = element_size.min(64);
                let slice = std::slice::from_raw_parts(p, len);
                let hex: String = slice.iter().map(|b| format!("{:02x}", b)).collect();
                json!({ "raw_bytes_hex": hex, "element_size": element_size })
            }
        }
    }
}
