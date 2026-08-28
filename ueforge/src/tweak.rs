//! Unified `TweakDef`. The static catalog form of the data-table /
//! class-field write surface. Subsumes the shape of
//! [`crate::stacks::StackDef`] + [`crate::difficulty::DifficultyDef`]
//! + ad-hoc [`crate::data_table::NamedFieldTweak`] /
//! [`crate::data_table::ClassNamedFieldTweak`] declarations.
//!
//! ```text
//! K8s slot: Def=TweakDef, Registry=TweakRegistry,
//!           Instance=per-row / per-instance captured vanilla in
//!                    data_table's dynamic registries,
//!           Controller=TweakDef::apply / TweakDef::revert
//! ```
//!
//! Every UE5 mod ends up writing one or more primitive fields on
//! either:
//!
//! - **Data-table rows** (every row in `DT_Materials.MaxCanStack`),
//!   or
//! - **Class instances** (every `UInventoryComponent.SlotCount`).
//!
//! Today three shapes own slices of this surface (StackDef,
//! DifficultyDef, NamedFieldTweak). `TweakDef` unifies them: one
//! Def shape per knob, runtime-tunable atomic value, kind +
//! operation enums so the dispatch goes through the shared
//! `data_table::dynamic_apply_*` (data-table targets) /
//! `tweak::dynamic_class_apply_*` (class targets) registries.
//! That means a single source of truth for captured vanilla +
//! revert state across the framework and per-mod catalogs.
//!
//! ```ignore
//! use ueforge::tweak::{TweakDef, TweakRegistry, TweakOp};
//!
//! static STACK_MATERIALS: TweakDef = TweakDef::data_table_i32(
//!     "stack_materials",
//!     "DT_Materials", "MaxCanStack",
//!     TweakOp::Multiply, 4,
//! );
//!
//! static BACKPACK_SLOTS: TweakDef = TweakDef::class_i32(
//!     "backpack_slots",
//!     "InventoryComponent", "SlotCount",
//!     TweakOp::Set, 40,
//! );
//!
//! pub static TWEAKS: TweakRegistry = TweakRegistry::new(&[
//!     &STACK_MATERIALS,
//!     &BACKPACK_SLOTS,
//! ]);
//!
//! // From `on_unreal_init` worker (after discovery::run_at_load):
//! let _ = TWEAKS.apply_all();
//!
//! // From the ImGui tab:
//! let mut m = STACK_MATERIALS.load_i32();
//! if ui::slider_i32("Multiplier", &mut m, 1, 64) {
//!     STACK_MATERIALS.store_i32(m);
//!     let _ = STACK_MATERIALS.apply();
//! }
//! ```
//!
//! Migration plan: new game tweaks declare `TweakDef`. Existing
//! `StackDef` / `DifficultyDef` keep working. Lift to `TweakDef`
//! when each is touched. The dynamic vanilla registries are the
//! same backend, so a mixed catalog (some StackDef, some TweakDef
//! targeting the same field) shares state correctly.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::data_table::{
    dynamic_apply_f32, dynamic_apply_i32, dynamic_apply_u32, dynamic_revert_one, resolve_field,
};

/// What the tweak writes to. Mirrors the two universal UE write
/// targets: a row inside a `UDataTable`, or a field on every live
/// instance of a `UClass`.
#[derive(Debug, Clone, Copy)]
pub enum TweakTarget {
    /// Every row of a named `UDataTable`'s `RowMap`, at the
    /// FProperty-resolved offset of `field`.
    DataTable {
        /// UE short name of the `UDataTable` instance.
        table: &'static str,
        /// FProperty name on the row struct.
        field: &'static str,
    },
    /// Every live instance of a named `UClass` (including the CDO),
    /// at the FProperty-resolved offset of `field`.
    Class {
        /// UE short name of the `UClass`.
        class: &'static str,
        /// FProperty name on the class.
        field: &'static str,
    },
}

/// Primitive type the write targets. Determines how
/// `current_value_bits` is interpreted + which dynamic registry
/// the apply path routes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TweakKind {
    I32,
    F32,
    U32,
}

impl TweakKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TweakKind::I32 => "i32",
            TweakKind::F32 => "f32",
            TweakKind::U32 => "u32",
        }
    }
}

/// Operation applied to each row/instance's captured vanilla.
/// Always re-bases on vanilla so repeated applies are idempotent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TweakOp {
    /// Write `value`, ignoring vanilla.
    Set,
    /// Write `vanilla * value` (vanilla unchanged after).
    Multiply,
    /// Write `vanilla + value`.
    Add,
}

impl TweakOp {
    pub fn as_str(self) -> &'static str {
        match self {
            TweakOp::Set => "set",
            TweakOp::Multiply => "multiply",
            TweakOp::Add => "add",
        }
    }
}

/// Unified tweak Def. Static catalog row + runtime-tunable
/// atomic value. Apply goes through the data_table dynamic
/// registries so captured vanilla survives across reapplies and
/// `tweak_revert` ops.
pub struct TweakDef {
    pub id: &'static str,
    pub target: TweakTarget,
    pub kind: TweakKind,
    pub op: TweakOp,
    /// Initial value bits, set at const construction; never
    /// mutated after.
    pub default_value_bits: u64,
    /// Current value bits. Sliders / hot-reload may mutate.
    pub current_value_bits: AtomicU64,
}

impl TweakDef {
    /// Construct a data-table i32 tweak. `value` is the literal
    /// argument for the operation (e.g. multiplier for
    /// `TweakOp::Multiply`).
    pub const fn data_table_i32(
        id: &'static str,
        table: &'static str,
        field: &'static str,
        op: TweakOp,
        value: i32,
    ) -> Self {
        let bits = value as u32 as u64;
        Self {
            id,
            target: TweakTarget::DataTable { table, field },
            kind: TweakKind::I32,
            op,
            default_value_bits: bits,
            current_value_bits: AtomicU64::new(bits),
        }
    }

    pub const fn data_table_f32(
        id: &'static str,
        table: &'static str,
        field: &'static str,
        op: TweakOp,
        value: f32,
    ) -> Self {
        let bits = value.to_bits() as u64;
        Self {
            id,
            target: TweakTarget::DataTable { table, field },
            kind: TweakKind::F32,
            op,
            default_value_bits: bits,
            current_value_bits: AtomicU64::new(bits),
        }
    }

    pub const fn data_table_u32(
        id: &'static str,
        table: &'static str,
        field: &'static str,
        op: TweakOp,
        value: u32,
    ) -> Self {
        let bits = value as u64;
        Self {
            id,
            target: TweakTarget::DataTable { table, field },
            kind: TweakKind::U32,
            op,
            default_value_bits: bits,
            current_value_bits: AtomicU64::new(bits),
        }
    }

    pub const fn class_i32(
        id: &'static str,
        class: &'static str,
        field: &'static str,
        op: TweakOp,
        value: i32,
    ) -> Self {
        let bits = value as u32 as u64;
        Self {
            id,
            target: TweakTarget::Class { class, field },
            kind: TweakKind::I32,
            op,
            default_value_bits: bits,
            current_value_bits: AtomicU64::new(bits),
        }
    }

    pub const fn class_f32(
        id: &'static str,
        class: &'static str,
        field: &'static str,
        op: TweakOp,
        value: f32,
    ) -> Self {
        let bits = value.to_bits() as u64;
        Self {
            id,
            target: TweakTarget::Class { class, field },
            kind: TweakKind::F32,
            op,
            default_value_bits: bits,
            current_value_bits: AtomicU64::new(bits),
        }
    }

    pub const fn class_u32(
        id: &'static str,
        class: &'static str,
        field: &'static str,
        op: TweakOp,
        value: u32,
    ) -> Self {
        let bits = value as u64;
        Self {
            id,
            target: TweakTarget::Class { class, field },
            kind: TweakKind::U32,
            op,
            default_value_bits: bits,
            current_value_bits: AtomicU64::new(bits),
        }
    }

    pub fn load_i32(&self) -> i32 {
        self.current_value_bits.load(Ordering::Relaxed) as u32 as i32
    }
    pub fn load_u32(&self) -> u32 {
        self.current_value_bits.load(Ordering::Relaxed) as u32
    }
    pub fn load_f32(&self) -> f32 {
        f32::from_bits(self.current_value_bits.load(Ordering::Relaxed) as u32)
    }

    pub fn store_i32(&self, v: i32) {
        self.current_value_bits
            .store(v as u32 as u64, Ordering::Relaxed);
    }
    pub fn store_u32(&self, v: u32) {
        self.current_value_bits.store(v as u64, Ordering::Relaxed);
    }
    pub fn store_f32(&self, v: f32) {
        self.current_value_bits
            .store(v.to_bits() as u64, Ordering::Relaxed);
    }

    /// Reset to default + re-apply. For "restore defaults"
    /// buttons; the captured vanilla is left intact so a future
    /// op back to a tuned value still re-bases correctly.
    pub fn reset_to_default(&self) -> Result<usize, String> {
        self.current_value_bits
            .store(self.default_value_bits, Ordering::Relaxed);
        self.apply()
    }

    /// Apply the tweak: walk every target row / instance,
    /// capture vanilla on first sight, write the operation
    /// result. Idempotent on re-apply (always re-bases on
    /// captured vanilla). Returns rows / instances touched.
    pub fn apply(&self) -> Result<usize, String> {
        match self.target {
            TweakTarget::DataTable { table, field } => self.apply_data_table(table, field),
            TweakTarget::Class { class, field } => self.apply_class(class, field),
        }
    }

    fn apply_data_table(&self, table: &str, field: &str) -> Result<usize, String> {
        match self.kind {
            TweakKind::I32 => {
                let v = self.load_i32();
                let op = self.op;
                dynamic_apply_i32(
                    table,
                    field,
                    move |vanilla| apply_i32(vanilla, op, v),
                    move |vanilla| skip_i32(vanilla, op),
                )
            }
            TweakKind::U32 => {
                let v = self.load_u32();
                let op = self.op;
                dynamic_apply_u32(
                    table,
                    field,
                    move |vanilla| apply_u32(vanilla, op, v),
                    move |vanilla| skip_u32(vanilla, op),
                )
            }
            TweakKind::F32 => {
                let v = self.load_f32();
                let op = self.op;
                dynamic_apply_f32(
                    table,
                    field,
                    move |vanilla| apply_f32(vanilla, op, v),
                    move |vanilla| !vanilla.is_finite(),
                )
            }
        }
    }

    fn apply_class(&self, class: &str, field: &str) -> Result<usize, String> {
        let (offset, _size) =
            crate::data_table::resolve_class_field(class, field).ok_or_else(|| {
                format!(
                    "TweakDef '{}': class field '{class}.{field}' not in discovery cache",
                    self.id
                )
            })?;
        match self.kind {
            TweakKind::I32 => {
                let v = self.load_i32();
                let op = self.op;
                let tweak = dyn_class_i32(class, offset);
                tweak
                    .apply(
                        |_| true,
                        move |vanilla| {
                            if skip_i32(vanilla, op) {
                                None
                            } else {
                                Some(apply_i32(vanilla, op, v))
                            }
                        },
                    )
                    .map(|s| s.patched)
            }
            TweakKind::U32 => {
                let v = self.load_u32();
                let op = self.op;
                let tweak = dyn_class_u32(class, offset);
                tweak
                    .apply(
                        |_| true,
                        move |vanilla| {
                            if skip_u32(vanilla, op) {
                                None
                            } else {
                                Some(apply_u32(vanilla, op, v))
                            }
                        },
                    )
                    .map(|s| s.patched)
            }
            TweakKind::F32 => {
                let v = self.load_f32();
                let op = self.op;
                let tweak = dyn_class_f32(class, offset);
                tweak
                    .apply(
                        |_| true,
                        move |vanilla| {
                            if !vanilla.is_finite() {
                                None
                            } else {
                                Some(apply_f32(vanilla, op, v))
                            }
                        },
                    )
                    .map(|s| s.patched)
            }
        }
    }

    /// Revert this tweak only. For DataTable targets, removes
    /// from the dynamic registry's vanilla cache by writing
    /// captured-vanilla back. For Class targets, restores every
    /// captured instance.
    pub fn revert(&self) -> Result<usize, String> {
        match self.target {
            TweakTarget::DataTable { table, field } => Ok(dynamic_revert_one(table, field)),
            TweakTarget::Class { class, field } => {
                let (offset, _size) = crate::data_table::resolve_class_field(class, field)
                    .ok_or_else(|| {
                        format!(
                            "TweakDef '{}': class field '{class}.{field}' not in discovery cache",
                            self.id
                        )
                    })?;
                match self.kind {
                    TweakKind::I32 => dyn_class_i32(class, offset).revert(),
                    TweakKind::U32 => dyn_class_u32(class, offset).revert(),
                    TweakKind::F32 => dyn_class_f32(class, offset).revert(),
                }
            }
        }
    }

    /// How many vanilla baselines have been captured so far.
    /// Surfaces the underlying dynamic registry's count without
    /// requiring callers to plumb in their own counter.
    pub fn vanilla_count(&self) -> usize {
        match self.target {
            TweakTarget::DataTable { table, field } => {
                crate::data_table::dynamic_vanilla_count(table, field)
            }
            TweakTarget::Class { class, field } => {
                let Some((offset, _)) = crate::data_table::resolve_class_field(class, field) else {
                    return 0;
                };
                match self.kind {
                    TweakKind::I32 => dyn_class_i32(class, offset).vanilla_count(),
                    TweakKind::U32 => dyn_class_u32(class, offset).vanilla_count(),
                    TweakKind::F32 => dyn_class_f32(class, offset).vanilla_count(),
                }
            }
        }
    }

    /// Quick `(offset, element_size)` peek through the discovery
    /// cache for status surfaces. None when the cache is empty or
    /// the target field can't be resolved.
    pub fn resolved(&self) -> Option<(usize, u32)> {
        match self.target {
            TweakTarget::DataTable { table, field } => {
                resolve_field(table, field).map(|(off, sz, _class)| (off, sz))
            }
            TweakTarget::Class { class, field } => {
                crate::data_table::resolve_class_field(class, field)
            }
        }
    }
}

#[inline]
fn apply_i32(vanilla: i32, op: TweakOp, v: i32) -> i32 {
    match op {
        TweakOp::Set => v,
        TweakOp::Multiply => vanilla.saturating_mul(v),
        TweakOp::Add => vanilla.saturating_add(v),
    }
}

#[inline]
fn apply_u32(vanilla: u32, op: TweakOp, v: u32) -> u32 {
    match op {
        TweakOp::Set => v,
        TweakOp::Multiply => vanilla.saturating_mul(v),
        TweakOp::Add => vanilla.saturating_add(v),
    }
}

#[inline]
fn apply_f32(vanilla: f32, op: TweakOp, v: f32) -> f32 {
    match op {
        TweakOp::Set => v,
        TweakOp::Multiply => vanilla * v,
        TweakOp::Add => vanilla + v,
    }
}

#[inline]
fn skip_i32(vanilla: i32, op: TweakOp) -> bool {
    matches!(op, TweakOp::Multiply) && vanilla == 0
}

#[inline]
fn skip_u32(vanilla: u32, op: TweakOp) -> bool {
    matches!(op, TweakOp::Multiply) && vanilla == 0
}

// ---- Workspace-standard registry wrapper ---------------------------------

/// Slice-of-refs `TweakRegistry`. Mirrors `StackRegistry` /
/// `DifficultyRegistry` / `DataTableRegistry` shape.
pub struct TweakRegistry {
    entries: &'static [&'static TweakDef],
}

impl TweakRegistry {
    pub const fn new(entries: &'static [&'static TweakDef]) -> Self {
        Self { entries }
    }

    pub fn def(&self, id: &str) -> Option<&'static TweakDef> {
        for t in self.entries {
            if t.id == id {
                return Some(*t);
            }
        }
        None
    }

    pub fn entries(&self) -> &'static [&'static TweakDef] {
        self.entries
    }

    pub fn iter(&self) -> std::slice::Iter<'_, &'static TweakDef> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Apply every registered tweak. Returns
    /// `Vec<(id, Result<rows_touched, err>)>` so a partial
    /// success (some fields not yet in discovery) is visible.
    pub fn apply_all(&self) -> Vec<(&'static str, Result<usize, String>)> {
        let mut out = Vec::with_capacity(self.entries.len());
        for def in self.entries {
            out.push((def.id, def.apply()));
        }
        out
    }

    /// Revert every registered tweak. Each Def's revert path
    /// uses the same dynamic registry the apply path used, so
    /// vanilla restoration is consistent.
    pub fn revert_all(&self) -> Vec<(&'static str, Result<usize, String>)> {
        let mut out = Vec::with_capacity(self.entries.len());
        for def in self.entries {
            out.push((def.id, def.revert()));
        }
        out
    }
}

impl<'a> IntoIterator for &'a TweakRegistry {
    type Item = &'static TweakDef;
    type IntoIter = std::iter::Copied<std::slice::Iter<'a, &'static TweakDef>>;
    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter().copied()
    }
}

// ---- Dynamic class-field registry ---------------------------------------
//
// Mirror of `data_table::DYN_I32/F32/U32` for ClassFieldTweak.
// Keyed by (class_name, offset) so multiple Defs targeting the
// same (class, field) share their captured-vanilla state. The
// class_name string is leaked to satisfy ClassFieldTweak's
// `&'static str` requirement; one leak per unique class.

use std::collections::HashMap;
use std::sync::OnceLock;

use parking_lot::Mutex as PlMutex;

use crate::ue::class_tweak::ClassFieldTweak;

type ClassRegistry<T> = PlMutex<HashMap<(String, usize), &'static ClassFieldTweak<T>>>;

static DYN_CLASS_I32: OnceLock<ClassRegistry<i32>> = OnceLock::new();
static DYN_CLASS_F32: OnceLock<ClassRegistry<f32>> = OnceLock::new();
static DYN_CLASS_U32: OnceLock<ClassRegistry<u32>> = OnceLock::new();

fn class_registry<T: Copy + PartialEq + Send + 'static>(
    cell: &'static OnceLock<ClassRegistry<T>>,
) -> &'static ClassRegistry<T> {
    cell.get_or_init(|| PlMutex::new(HashMap::new()))
}

fn dyn_class<T: Copy + PartialEq + Send + 'static>(
    reg: &'static ClassRegistry<T>,
    class: &str,
    offset: usize,
) -> &'static ClassFieldTweak<T> {
    let key = (class.to_string(), offset);
    if let Some(t) = reg.lock().get(&key) {
        return *t;
    }
    let class_static: &'static str = Box::leak(class.to_string().into_boxed_str());
    let tweak: &'static ClassFieldTweak<T> =
        Box::leak(Box::new(ClassFieldTweak::new(class_static, offset)));
    reg.lock().insert(key, tweak);
    tweak
}

fn dyn_class_i32(class: &str, offset: usize) -> &'static ClassFieldTweak<i32> {
    dyn_class(class_registry::<i32>(&DYN_CLASS_I32), class, offset)
}
fn dyn_class_u32(class: &str, offset: usize) -> &'static ClassFieldTweak<u32> {
    dyn_class(class_registry::<u32>(&DYN_CLASS_U32), class, offset)
}
fn dyn_class_f32(class: &str, offset: usize) -> &'static ClassFieldTweak<f32> {
    dyn_class(class_registry::<f32>(&DYN_CLASS_F32), class, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn const_constructible() {
        // Verifies all six constructors can be used in const context.
        const A: TweakDef = TweakDef::data_table_i32("a", "T", "F", TweakOp::Multiply, 4);
        const B: TweakDef = TweakDef::data_table_f32("b", "T", "F", TweakOp::Set, 1.5);
        const C: TweakDef = TweakDef::data_table_u32("c", "T", "F", TweakOp::Add, 7);
        const D: TweakDef = TweakDef::class_i32("d", "C", "F", TweakOp::Set, 40);
        const E: TweakDef = TweakDef::class_f32("e", "C", "F", TweakOp::Multiply, 0.5);
        const F: TweakDef = TweakDef::class_u32("f", "C", "F", TweakOp::Set, 0xFFFFFFFF);
        assert_eq!(A.kind, TweakKind::I32);
        assert_eq!(B.kind, TweakKind::F32);
        assert_eq!(C.kind, TweakKind::U32);
        assert_eq!(D.kind, TweakKind::I32);
        assert_eq!(E.kind, TweakKind::F32);
        assert_eq!(F.kind, TweakKind::U32);
    }

    #[test]
    fn load_store_round_trip() {
        static T: TweakDef = TweakDef::data_table_f32("x", "T", "F", TweakOp::Multiply, 2.5);
        assert_eq!(T.load_f32(), 2.5);
        T.store_f32(3.75);
        assert_eq!(T.load_f32(), 3.75);
    }

    #[test]
    fn op_math() {
        assert_eq!(apply_i32(10, TweakOp::Set, 5), 5);
        assert_eq!(apply_i32(10, TweakOp::Multiply, 5), 50);
        assert_eq!(apply_i32(10, TweakOp::Add, 5), 15);

        assert_eq!(apply_f32(2.0, TweakOp::Multiply, 0.5), 1.0);
        assert!(skip_i32(0, TweakOp::Multiply));
        assert!(!skip_i32(0, TweakOp::Set));
        assert!(!skip_i32(0, TweakOp::Add));
    }

    #[test]
    fn registry_lookup() {
        static T1: TweakDef = TweakDef::data_table_i32("t1", "T", "F", TweakOp::Set, 1);
        static T2: TweakDef = TweakDef::class_f32("t2", "C", "F", TweakOp::Multiply, 2.0);
        static REG: TweakRegistry = TweakRegistry::new(&[&T1, &T2]);
        assert_eq!(REG.len(), 2);
        assert!(REG.def("t1").is_some());
        assert!(REG.def("t2").is_some());
        assert!(REG.def("nope").is_none());
        let ids: Vec<&str> = REG.iter().map(|d| d.id).collect();
        assert_eq!(ids, vec!["t1", "t2"]);
    }
}
