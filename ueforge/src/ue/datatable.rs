//! `UDataTable` row helpers.
//!
//! A `UDataTable` carries a `TMap<FName, uint8*>` named `RowMap`
//! at `+0x30`. Each value pointer points at the row struct's bytes,
//! which the caller decodes with a `#[repr(C)]` struct mirroring
//! the row's `RowStruct` layout (per-game).
//!
//! Helpers here cover the two common shapes:
//! - "I have an FName, give me the row pointer."
//! - "Give me every row as `(name, ptr)` so I can scan."
//!
//! Plus a class-name-to-instance helper built on `walk_class`'s
//! engine pieces so a test can do
//!
//! ```ignore
//! let dt = ueforge::ue::datatable::find_by_short_name("DT_Materials")
//!     .ok_or("no DT_Materials")?;
//! let row = ueforge::ue::datatable::row_value_by_fname(dt, my_fname)
//!     .ok_or("row not in table")?;
//! // SAFETY: row points at FRowStruct bytes (per game)
//! let v: u32 = unsafe { (row.add(STACK_OFFSET) as *const u32).read_unaligned() };
//! ```

use std::collections::HashMap;
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::ue::offsets::datatable as off;
use crate::ue::{self, UObject, fname::FName, tmap};

/// Pointer to the row struct's bytes for the given FName, or
/// `None` if the table has no such row.
///
/// # Safety
/// `table` must be a live `UDataTable`. The returned pointer is
/// only valid until GObjects / the table is mutated.
pub unsafe fn row_value_by_fname(table: &UObject, row_name: FName) -> Option<*const u8> {
    // SAFETY: FName is an 8-byte struct laid out identically to
    // u64 (comparison_index: i32 + number: u32); transmute_copy
    // round-trips the bytes.
    let key: u64 = unsafe { std::mem::transmute_copy(&row_name) };
    // SAFETY: caller guarantees `table` is a live UDataTable;
    // off::ROW_MAP is the configured offset of the TMap; the
    // tmap walker's contract matches.
    unsafe { tmap::find_value_by_fname_key(table, off::ROW_MAP, key) }
}

/// Iterate every `(FName-as-u64, row_ptr)` pair in the table's
/// RowMap. Free / unallocated slots are filtered by null value
/// pointer; callers should additionally validate the FName by
/// resolving it against the global name resolver before treating
/// the key as canonical.
///
/// # Safety
/// `table` must be a live `UDataTable`.
pub unsafe fn iter_rows(table: &UObject) -> impl Iterator<Item = (u64, *const u8)> + '_ {
    // SAFETY: caller's `unsafe fn` contract; tmap::slots returns
    // valid 24-byte slot pointers within the TSparseArray.
    let inner = unsafe { tmap::slots(table, off::ROW_MAP) };
    inner.filter_map(move |(_, element)| {
        // SAFETY: slot pointer is valid for 24 bytes; first 8 are
        // the FName-as-u64 key, +PAIR_VALUE is the value pointer.
        // Free slots filtered out via the null-value check.
        unsafe {
            let key: u64 = (element as *const u64).read_unaligned();
            let value: *const u8 = (element.add(crate::ue::offsets::tmap::PAIR_VALUE)
                as *const *const u8)
                .read_unaligned();
            if value.is_null() {
                None
            } else {
                Some((key, value))
            }
        }
    })
}

/// Build a map of row name (String) to raw FName key (u64) for
/// every row in the table. Useful for resolving item names to
/// FNames for TArray entry construction.
///
/// # Safety
/// `table` must be a live `UDataTable`.
pub unsafe fn row_name_map(table: &UObject) -> HashMap<String, u64> {
    let rt = match ue::try_runtime() {
        Some(r) => r,
        None => return HashMap::new(),
    };
    let mut map = HashMap::new();
    unsafe {
        for (fname_key, _row_ptr) in iter_rows(table) {
            let fname = FName::from_u64(fname_key);
            let name = rt.name_resolver.to_string(fname);
            map.insert(name, fname_key);
        }
    }
    map
}

/// Spawn a worker that polls for a `UDataTable` by short name
/// and runs `on_ready` once on the first sighting. Mod feature
/// code typically calls this from `ModInfo::on_unreal_init` so
/// DT mutations land BEFORE any UI widget / actor caches a row
/// copy. See ueforge/README.md "Pattern: DataTable mutation
/// timing" for why timing matters.
///
/// `on_ready` runs on the worker thread (not the game thread).
/// Reading + writing field bytes via raw pointer arithmetic is
/// safe enough for one-shot DT mutations during init. UE isn't
/// iterating those rows yet. For anything more invasive, queue
/// to a game-thread drain via `ueforge::Queue`.
///
/// The worker exits after one successful run, or after
/// `timeout` elapses without finding the DT.
pub fn on_first_sight<F>(name: &'static str, timeout: Duration, on_ready: F)
where
    F: FnOnce(&'static UObject) + Send + 'static,
{
    let interval = Duration::from_millis(250);
    let thread_name = format!("ueforge-dt-{name}");
    let _ = thread::Builder::new().name(thread_name).spawn(move || {
        let started = Instant::now();
        loop {
            if let Some(dt) = find_by_short_name(name) {
                on_ready(dt);
                return;
            }
            if started.elapsed() > timeout {
                crate::log::log(format_args!(
                    "ueforge::datatable::on_first_sight: timed out waiting \
                     for '{name}' after {:?}",
                    timeout
                ));
                return;
            }
            thread::sleep(interval);
        }
    });
}

// ---- FieldTweak<T> --------------------------------------------------------
//
// Generic primitive for "snapshot vanilla, mutate per row, re-apply on
// demand without compounding". Used by stack-size mods, recipe yield
// tweaks, drop-rate adjusters, anything that mutates a single fixed
// field on every row of a known DataTable.

/// Tweak helper: snapshots vanilla `T` per row on first apply,
/// then writes `transform(vanilla)` to each row's field at
/// `offset`. Subsequent applies recompute from the vanilla
/// snapshot, so changing the transform (e.g. switching a slider
/// from 4x to 8x) doesn't compound the previous result.
///
/// Typical use:
///
/// ```ignore
/// static STACKS: FieldTweak<i32> =
///     FieldTweak::new("DT_Materials", 0x48);
///
/// // From on_unreal_init:
/// STACKS.apply_when_ready(
///     Duration::from_secs(30),
///     || (|v: i32| v.saturating_mul(current_multiplier())),
///     || (|v: i32| v <= 1),
/// );
///
/// // From a tab button click:
/// STACKS.apply(
///     |v| v.saturating_mul(current_multiplier()),
///     |v| v <= 1,
/// )?;
/// ```
///
/// `T` must be `Copy + PartialEq` (we read-compare-write each
/// row) and `Send + 'static` (we share state across threads).
/// Common Ts: `i32`, `f32`, `u8`. Larger row fields (FName,
/// FString, FVector) need bespoke logic. This primitive is for
/// the simple primitive-typed case.
pub struct FieldTweak<T: Copy + PartialEq + Send + 'static> {
    table_name: &'static str,
    offset: usize,
    vanilla: OnceLock<Mutex<HashMap<u64, T>>>,
}

impl<T: Copy + PartialEq + Send + 'static> FieldTweak<T> {
    pub const fn new(table_name: &'static str, offset: usize) -> Self {
        Self {
            table_name,
            offset,
            vanilla: OnceLock::new(),
        }
    }

    /// Find the DT and apply once. `transform` and `skip_if` are
    /// called with the VANILLA value (snapshotted on first sight
    /// of each row), not the current mutated value.
    pub fn apply<F, S>(&self, transform: F, skip_if: S) -> Result<usize, String>
    where
        F: Fn(T) -> T,
        S: Fn(T) -> bool,
    {
        let table = find_by_short_name(self.table_name)
            .ok_or_else(|| format!("DataTable '{}' not found", self.table_name))?;
        self.apply_to(table, transform, skip_if)
    }

    /// Apply to a specific DT instance. Used internally by
    /// `apply` and `apply_when_ready`; exposed for callers that
    /// already have the table reference.
    pub fn apply_to<F, S>(&self, table: &UObject, transform: F, skip_if: S) -> Result<usize, String>
    where
        F: Fn(T) -> T,
        S: Fn(T) -> bool,
    {
        let vanilla = self.vanilla.get_or_init(|| Mutex::new(HashMap::new()));
        let mut van = vanilla.lock();
        let mut touched = 0usize;
        // SAFETY: `table` is the caller's resolved UDataTable
        // (live UObject); iter_rows yields (fname_u64, row_ptr)
        // pairs valid for the duration of this walk; field_ptr
        // math at self.offset stays within the row's allocation
        // (row layout came from the engine's RowStruct).
        unsafe {
            for (fname, row_ptr) in iter_rows(table) {
                if row_ptr.is_null() {
                    continue;
                }
                let field_ptr = row_ptr.add(self.offset) as *mut T;
                let cur = field_ptr.read_unaligned();
                let vanilla_val = *van.entry(fname).or_insert(cur);
                if skip_if(vanilla_val) {
                    continue;
                }
                let target = transform(vanilla_val);
                if cur != target {
                    field_ptr.write_unaligned(target);
                }
                touched += 1;
            }
        }
        Ok(touched)
    }

    /// Spawn a background worker that polls for the table (via
    /// [`on_first_sight`]) and applies once on first sighting.
    /// Mod feature code calls this from `on_unreal_init` so the
    /// mutation lands BEFORE any UI / actor caches a row copy.
    pub fn apply_when_ready<F, S>(&'static self, timeout: Duration, transform: F, skip_if: S)
    where
        F: Fn(T) -> T + Send + 'static,
        S: Fn(T) -> bool + Send + 'static,
    {
        on_first_sight(self.table_name, timeout, move |table| {
            match self.apply_to(table, &transform, &skip_if) {
                Ok(n) => crate::log::log(format_args!(
                    "FieldTweak<{}> applied to {n} rows of {}",
                    std::any::type_name::<T>(),
                    self.table_name
                )),
                Err(e) => crate::log::log(format_args!(
                    "FieldTweak<{}> apply failed on {}: {e}",
                    std::any::type_name::<T>(),
                    self.table_name
                )),
            }
        });
    }

    /// How many vanilla baselines have been snapshotted so far.
    /// Useful for status displays in mod UIs.
    pub fn vanilla_count(&self) -> usize {
        self.vanilla.get().map(|m| m.lock().len()).unwrap_or(0)
    }

    /// Reset every captured row back to its vanilla value.
    /// Equivalent to `apply(|v| v, |_| false)`.
    pub fn revert(&self) -> Result<usize, String> {
        self.apply(|v| v, |_| false)
    }

    pub fn table_name(&self) -> &'static str {
        self.table_name
    }
    pub fn offset(&self) -> usize {
        self.offset
    }
}

/// Find the first non-CDO `UDataTable` instance whose short name
/// matches `name`. Walks GObjects on the first miss; the result is
/// cached by name for the rest of the process. Mirrors
/// [`crate::ue::find_class_fast`]'s shape: UDataTable instances
/// are stable in GObjects for the process lifetime, so a hit is
/// permanent. Misses (table not loaded yet) are NOT cached;
/// a later call after the table loads walks again and finds it.
pub fn find_by_short_name(name: &str) -> Option<&'static UObject> {
    let cache = table_cache();
    if let Some(t) = cache.read().get(name) {
        return Some(*t);
    }
    let rt = ue::try_runtime()?;
    let class = ue::find_class_fast("DataTable")?;
    // SAFETY: rt was returned by try_runtime() which is set once
    // by detect_and_init; its image_base + platform_offsets were
    // validated at runtime init.
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return None;
    }
    for obj in view.iter() {
        if !obj.is_a(class) {
            continue;
        }
        if obj.is_default_object() {
            continue;
        }
        if obj.name() == name {
            // SAFETY: lifetime extension. UDataTables are stable in
            // GObjects for the process lifetime. See
            // selector::resolve_generic for the broader contract.
            let extended: &'static UObject =
                unsafe { std::mem::transmute::<&UObject, &'static UObject>(obj) };
            cache.write().insert(name.to_string(), extended);
            return Some(extended);
        }
    }
    None
}

static TABLE_CACHE: OnceLock<parking_lot::RwLock<HashMap<String, &'static UObject>>> =
    OnceLock::new();
fn table_cache() -> &'static parking_lot::RwLock<HashMap<String, &'static UObject>> {
    TABLE_CACHE.get_or_init(|| parking_lot::RwLock::new(HashMap::with_capacity(64)))
}
