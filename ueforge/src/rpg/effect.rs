//! Effect trait + per-operation struct types.
//!
//! Per the workspace
//! [composition model](../../docs/architecture.md), each thing
//! we research and figure out how to do in the game is an
//! `Effect`. Skills are compositions of Effects with parameters.
//! We do each operation ONCE; every game's catalog rows compose
//! the same Effect types.
//!
//! ```text
//! K8s slot: Def=EffectDef { kind, imp: &'static dyn Effect },
//!           Registry=catalog (each SkillDef carries one EffectDef inline),
//!           Instance=&'static dyn Effect resolved per call,
//!           Controller=Effect::apply / Effect::format
//! ```
//!
//! ## Adding a new operation
//!
//! 1. Define a struct with the operation's parameters as fields
//!    (`pub struct MyEffect { pub offset: usize, pub max_bonus: f32, ... }`).
//! 2. `impl Effect for MyEffect`.
//! 3. Declare a `static MY_INSTANCE: MyEffect = MyEffect { ... };` per use.
//! 4. Reference it from a catalog row:
//!    `effect: EffectDef::new("MyKind", &MY_INSTANCE)`.
//!
//! Standard operations the framework ships are below
//! ([`PlayerFloatEffect`], [`SubcomponentMultiplyEffect`], etc.).
//! Game-specific operations live in the game crate and follow
//! the same pattern.

use std::ffi::c_void;

use crate::rpg::format::{self, PercentFormat, format_pct};
use crate::rpg::progress::sqrt_progress;
use crate::rpg::trigger::UeEngine;
use crate::rpg::vanilla::VanillaCache;
use crate::ue::{ClassRef, PlayerRef, TypedField, UObject};

// The `Effect` trait + `EffectDef` shape live in modforge::rpg
// as engine-generic types. UE call sites use the modforge
// trait directly + a `UeEngine` type parameter on each impl;
// catalog rows use the type alias below.
pub use modforge::rpg::Effect;

/// UE-side type alias for the engine-generic
/// [`modforge::rpg::EffectDef`]. Catalog rows declare
/// `EffectDef::new("Kind", &INSTANCE)` without naming the
/// engine type parameter.
pub type EffectDef = modforge::rpg::EffectDef<UeEngine>;

// =====================================================================
// Standard Effect implementations. The canonical UE5 RPG operation
// library. Each used to be a variant of `StandardEffect`; now each is
// its own type.
// =====================================================================

/// Direct `f32` write at a fixed offset on the player class
/// (CDO + live pawns). Final value: `base + max_bonus * progress`.
///
/// Used for fields directly on the pawn class.
/// `ASurvivalCharacter.CustomDamageMultiplier` etc.
pub struct PlayerFloatEffect {
    pub player: &'static PlayerRef,
    pub offset: TypedField<f32>,
    pub base: f32,
    pub max_bonus: f32,
    pub format: PercentFormat,
}

impl Effect<UeEngine> for PlayerFloatEffect {
    fn apply(&self, level: u32, max_level: u32, _ctx: &crate::rpg::TriggerCtx<'_>) {
        let progress = sqrt_progress(level, max_level);
        let value = self.base + self.max_bonus * progress;
        self.player.for_each_cdo(|cdo| {
            // SAFETY: TypedField::write requires the offset to be
            // a valid f32 within the object. PlayerRef yields
            // resolved player CDOs / live pawns.
            unsafe { self.offset.write(cdo, value) };
        });
        self.player.for_each_live(|p| {
            // SAFETY: see CDO arm.
            unsafe { self.offset.write(p, value) };
        });
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        format_pct(self.base, self.max_bonus, level, max_level, &self.format)
    }
}

/// `f32` write on a player **subcomponent** (HealthComponent,
/// CharMovementComponent, etc.) reached by following a
/// `*mut UObject` pointer at `component_offset` from the pawn.
/// Final value: `base + max_bonus * progress`.
pub struct SubcomponentFloatEffect {
    pub player: &'static PlayerRef,
    pub component_offset: TypedField<*mut UObject>,
    pub field_offset: TypedField<f32>,
    pub base: f32,
    pub max_bonus: f32,
    pub format: PercentFormat,
}

impl Effect<UeEngine> for SubcomponentFloatEffect {
    fn apply(&self, level: u32, max_level: u32, _ctx: &crate::rpg::TriggerCtx<'_>) {
        let progress = sqrt_progress(level, max_level);
        let value = self.base + self.max_bonus * progress;
        let comp_offset = self.component_offset;
        let field_offset = self.field_offset;
        let mut walk = |obj: &UObject| {
            // SAFETY: TypedField::deref + write are sound when
            // the offset is a valid pointer field on the object.
            unsafe {
                if let Some(comp) = comp_offset.deref(obj) {
                    field_offset.write(comp, value);
                }
            }
        };
        self.player.for_each_cdo(&mut walk);
        self.player.for_each_live(&mut walk);
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        format_pct(self.base, self.max_bonus, level, max_level, &self.format)
    }
}

/// Additive `f32` on a subcomponent: capture vanilla on first
/// sight, write `vanilla + max_bonus * progress`. Used for
/// stacking-style fields (Max Health) where we want to ADD to
/// whatever the engine has, not REPLACE.
pub struct SubcomponentAdditiveEffect {
    pub player: &'static PlayerRef,
    pub component_offset: TypedField<*mut UObject>,
    pub field_offset: TypedField<f32>,
    pub max_bonus: f32,
    pub format_word: &'static str,
    pub vanilla: &'static VanillaCache<usize, f32>,
}

impl Effect<UeEngine> for SubcomponentAdditiveEffect {
    fn apply(&self, level: u32, max_level: u32, _ctx: &crate::rpg::TriggerCtx<'_>) {
        let progress = sqrt_progress(level, max_level);
        let off_key = self.field_offset.offset();
        let comp_offset = self.component_offset;
        let field_offset = self.field_offset;
        let vanilla = self.vanilla;
        let max_bonus = self.max_bonus;
        let mut walk = |obj: &UObject| {
            // SAFETY: TypedField ops; comp_offset.deref returns
            // None if the pointer field is null.
            unsafe {
                if let Some(comp) = comp_offset.deref(obj) {
                    let cur = field_offset.read(comp);
                    let baseline = if cur.is_finite() && cur != 0.0 {
                        vanilla.get_or_init(off_key, cur)
                    } else {
                        vanilla.get(off_key).unwrap_or(cur)
                    };
                    field_offset.write(comp, baseline + max_bonus * progress);
                }
            }
        };
        self.player.for_each_cdo(&mut walk);
        self.player.for_each_live(&mut walk);
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        format::format_additive_f32_as_int(
            self.max_bonus,
            level,
            max_level,
            self.format_word,
        )
    }
}

/// `u32` mask write on a subcomponent. When `level > 0`,
/// write `mask`; when `level == 0`, restore the captured
/// vanilla. Useful for binary-gate fields like
/// `RequiredDamageTypeFlags`.
pub struct SubcomponentU32MaskEffect {
    pub player: &'static PlayerRef,
    pub component_offset: TypedField<*mut UObject>,
    pub field_offset: TypedField<u32>,
    pub mask: u32,
    pub format: PercentFormat,
    pub vanilla: &'static VanillaCache<usize, u32>,
}

impl Effect<UeEngine> for SubcomponentU32MaskEffect {
    fn apply(&self, level: u32, _max_level: u32, _ctx: &crate::rpg::TriggerCtx<'_>) {
        let off_key = self.field_offset.offset();
        let comp_offset = self.component_offset;
        let field_offset = self.field_offset;
        let mask = self.mask;
        let vanilla = self.vanilla;
        let mut walk = |obj: &UObject| {
            // SAFETY: TypedField ops on the resolved component.
            unsafe {
                if let Some(comp) = comp_offset.deref(obj) {
                    let cur = field_offset.read(comp);
                    vanilla.get_or_init(off_key, cur);
                    let target = if level > 0 {
                        mask
                    } else {
                        vanilla.get(off_key).unwrap_or(cur)
                    };
                    field_offset.write(comp, target);
                }
            }
        };
        self.player.for_each_cdo(&mut walk);
        self.player.for_each_live(&mut walk);
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        // Boolean-style: at any non-zero level the gate is fully active.
        let bonus = if level > 0 { 1.0 } else { 0.0 };
        format_pct(0.0, bonus, level, max_level, &self.format)
    }
}

/// Multiply each of `offsets` on a player subcomponent by
/// `1 + max_bonus * progress`, scaled per-axis from the
/// captured vanilla. Used for movement skills that scale
/// multiple CharMovementComponent fields together.
pub struct SubcomponentMultiplyEffect {
    pub player: &'static PlayerRef,
    pub component_offset: TypedField<*mut UObject>,
    pub offsets: &'static [usize],
    pub max_bonus: f32,
    pub format_word: &'static str,
    pub vanilla: &'static VanillaCache<usize, f32>,
}

impl Effect<UeEngine> for SubcomponentMultiplyEffect {
    fn apply(&self, level: u32, max_level: u32, _ctx: &crate::rpg::TriggerCtx<'_>) {
        let progress = sqrt_progress(level, max_level);
        let mult = 1.0 + self.max_bonus * progress;
        let comp_offset = self.component_offset;
        let offsets = self.offsets;
        let vanilla = self.vanilla;
        let mut walk = |obj: &UObject| {
            // SAFETY: TypedField ops on the resolved component.
            unsafe {
                if let Some(comp) = comp_offset.deref(obj) {
                    for &off in offsets {
                        let f: TypedField<f32> = TypedField::at(off);
                        let cur = f.read(comp);
                        let baseline = if cur.is_finite() && cur != 0.0 {
                            vanilla.get_or_init(off, cur)
                        } else {
                            vanilla.get(off).unwrap_or(cur)
                        };
                        f.write(comp, baseline * mult);
                    }
                }
            }
        };
        self.player.for_each_cdo(&mut walk);
        self.player.for_each_live(&mut walk);
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        format::format_multiplier(self.max_bonus, level, max_level, self.format_word)
    }
}

/// Multiply offsets on every instance (CDO + non-CDO) of a
/// class. `(offset, exponent)` pairs let "boost" fields use
/// `+1.0` and "shrink" fields (regen tick rate, etc.) use
/// `-1.0`. Final value at offset = `vanilla * (1 + max_bonus *
/// progress)^exponent`.
pub struct ClassFieldsMultiplyEffect {
    pub class: &'static ClassRef,
    /// `(field_offset, exponent)` pairs.
    pub offsets: &'static [(usize, f32)],
    pub max_bonus: f32,
    pub format_word: &'static str,
    pub vanilla: &'static VanillaCache<usize, f32>,
}

impl Effect<UeEngine> for ClassFieldsMultiplyEffect {
    fn apply(&self, level: u32, max_level: u32, _ctx: &crate::rpg::TriggerCtx<'_>) {
        let progress = sqrt_progress(level, max_level);
        let mult = 1.0 + self.max_bonus * progress;
        let offsets = self.offsets;
        let vanilla = self.vanilla;
        self.class.for_each_any(|obj| {
            // SAFETY: TypedField::read / write on the iterated
            // class instance. ClassRef::for_each_any only yields
            // valid UObjects of the configured class.
            unsafe {
                for &(off, exp) in offsets {
                    let f: TypedField<f32> = TypedField::at(off);
                    let cur = f.read(obj);
                    let baseline = if cur.is_finite() && cur != 0.0 {
                        vanilla.get_or_init(off, cur)
                    } else {
                        vanilla.get(off).unwrap_or(cur)
                    };
                    f.write(obj, baseline * mult.powf(exp));
                }
            }
        });
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        format::format_multiplier(self.max_bonus, level, max_level, self.format_word)
    }
}

// `RuntimeEffect` moved to `modforge::rpg::std_effect` during
// Phase 0b row 16. Engine-agnostic; blanket `impl<E: Engine>
// Effect<E>`. Re-exported via `crate::rpg::RuntimeEffect`.
pub use modforge::rpg::std_effect::RuntimeEffect;

/// Apply a UE5 row-driven status effect to the player's
/// `UStatusEffectComponent`. Mutates the row's `Value` field to
/// `vanilla + (value_at_max - vanilla) * progress`, then invokes
/// the `function_name` UFunction on the player's component with
/// a `(table, row_fname)` row-handle parm.
///
/// The row identity (table + fname + value-offset + vanilla
/// cache) lives on the referenced [`StatusDef`](super::StatusDef)
/// so multiple Effects can target the same status.
pub struct StatusEffectApply {
    pub player: &'static PlayerRef,
    /// Identity of the status-effect row this Effect targets.
    pub status: &'static crate::rpg::StatusDef,
    /// `&UClass` of the player's status-effect component
    /// (e.g. "StatusEffectComponent" or
    /// "GearStatusEffectComponent" depending on game).
    pub component_class: &'static ClassRef,
    /// `*mut UObject` at this offset on the player IS the
    /// component instance.
    pub component_offset: TypedField<*mut UObject>,
    /// UFunction name on `component_class` to call.
    /// Typically `"CreateAndAddEffect"`.
    pub function_name: &'static str,
    /// Target value at level == max_level. At level == 0 the
    /// captured vanilla is restored.
    pub value_at_max: f32,
    pub format_word: &'static str,
}

impl Effect<UeEngine> for StatusEffectApply {
    fn apply(&self, level: u32, max_level: u32, _ctx: &crate::rpg::TriggerCtx<'_>) {
        let progress = sqrt_progress(level, max_level);
        let Some(table) = (self.status.table_finder)() else {
            crate::log!("rpg/effect: status-effect: table not loaded yet");
            return;
        };
        let row_fname_handle = crate::ue::FName::from_u64(self.status.row_fname);
        // SAFETY: row_value_by_fname does its own bounds check on
        // the table.
        let Some(row_ptr) =
            (unsafe { crate::ue::datatable::row_value_by_fname(table, row_fname_handle) })
        else {
            crate::log!(
                "rpg/effect: status-effect: row 0x{:016x} ({}) not found in table",
                self.status.row_fname,
                self.status.id
            );
            return;
        };

        // SAFETY: row_ptr was returned by the engine's data-table
        // walk; offset is the configured value-offset within the
        // row struct.
        let cur_val = unsafe {
            crate::ue::status_effect::read_row_value(row_ptr, self.status.value_offset)
        };
        let baseline = self.status.vanilla.get_or_init(self.status.row_fname, cur_val);

        let target = if level > 0 {
            baseline + (self.value_at_max - baseline) * progress
        } else {
            baseline
        };
        // SAFETY: see read above; same row_ptr + offset.
        unsafe {
            crate::ue::status_effect::write_row_value(
                row_ptr,
                self.status.value_offset,
                target,
            );
        }

        let Some(function) = self.component_class.find_function(self.function_name)
        else {
            crate::log!(
                "rpg/effect: status-effect: {} not found on {}",
                self.function_name,
                self.component_class.name()
            );
            return;
        };

        let comp_offset = self.component_offset;
        let table_ref = table;
        let row_fname = self.status.row_fname;
        self.player.for_each_live(|pawn| {
            // SAFETY: comp_offset.deref returns None if the
            // pointer field is null; otherwise it's a valid
            // UObject ref.
            if let Some(component) = unsafe { comp_offset.deref(pawn) } {
                // create_and_add_effect drives process_event
                // with the row handle.
                let _: *mut c_void = std::ptr::null_mut();
                crate::ue::status_effect::create_and_add_effect(
                    component, function, table_ref, row_fname,
                );
            }
        });
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        let progress = sqrt_progress(level, max_level);
        let pct = (progress * 100.0).round() as i32;
        let target = self.value_at_max * progress;
        format!("{pct}% {} (to value={target:.2})", self.format_word)
    }
}
