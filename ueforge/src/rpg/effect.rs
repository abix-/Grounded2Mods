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
//! ([`PlayerFloatEffect`], [`SubcomponentMultiplyEffect`],
//! [`SurvivalDrainEffect`], [`FallDamageReductionEffect`],
//! [`LifestealEffect`], [`ImpactReversalEffect`], etc.).
//! Game-specific operations live in the game crate and follow
//! the same pattern.

use std::{ffi::c_void, sync::OnceLock};

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
        format::format_additive_f32_as_int(self.max_bonus, level, max_level, self.format_word)
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

/// Reduce fall damage by updating the related player and game-mode
/// fields from their captured vanilla values.
pub struct FallDamageReductionEffect {
    pub player: &'static PlayerRef,
    pub ratio_offset: usize,
    pub take_fall_damage_offset: usize,
    pub min_velocity_offset: usize,
    pub game_mode_settings: &'static ClassRef,
    pub game_mode_multiplier_offset: usize,
    pub mode_manager_components: &'static ClassRef,
    pub mode_manager_multiplier_offset: usize,
    pub max_reduction: f32,
    pub min_velocity_multiplier_at_max: f32,
    pub disable_damage_at: f32,
    pub vanilla_ratio: &'static OnceLock<f32>,
    pub vanilla_min_velocity: &'static OnceLock<f32>,
    pub vanilla_game_mode_multiplier: &'static OnceLock<f32>,
    pub vanilla_mode_manager_multiplier: &'static OnceLock<f32>,
}

impl Effect<UeEngine> for FallDamageReductionEffect {
    fn apply(&self, level: u32, max_level: u32, _ctx: &crate::rpg::TriggerCtx<'_>) {
        let reduction = (self.max_reduction * sqrt_progress(level, max_level)).min(1.0);
        let cdo_count = self.player.for_each_cdo(|player_cdo| {
            let cur = crate::ue::field::read_f32(player_cdo, self.ratio_offset);
            if cur.is_finite() && cur > 0.0 {
                let _ = self.vanilla_ratio.set(cur);
            }
            let cur_min_velocity = crate::ue::field::read_f32(player_cdo, self.min_velocity_offset);
            if cur_min_velocity.is_finite() && cur_min_velocity > 0.0 {
                let _ = self.vanilla_min_velocity.set(cur_min_velocity);
            }
            if let Some(v) = self.vanilla_ratio.get().copied() {
                crate::ue::field::write_f32(player_cdo, self.ratio_offset, v * (1.0 - reduction));
            }
            if let Some(v) = self.vanilla_min_velocity.get().copied() {
                let boosted = v * (1.0 + reduction * (self.min_velocity_multiplier_at_max - 1.0));
                crate::ue::field::write_f32(player_cdo, self.min_velocity_offset, boosted);
            }
            crate::ue::field::write_bool(
                player_cdo,
                self.take_fall_damage_offset,
                reduction < self.disable_damage_at,
            );
        });
        let live_count = self.player.for_each_live(|player| {
            if let Some(v) = self.vanilla_ratio.get().copied() {
                crate::ue::field::write_f32(player, self.ratio_offset, v * (1.0 - reduction));
            }
            if let Some(v) = self.vanilla_min_velocity.get().copied() {
                let boosted = v * (1.0 + reduction * (self.min_velocity_multiplier_at_max - 1.0));
                crate::ue::field::write_f32(player, self.min_velocity_offset, boosted);
            }
            crate::ue::field::write_bool(
                player,
                self.take_fall_damage_offset,
                reduction < self.disable_damage_at,
            );
        });
        let gms_count = self.game_mode_settings.for_each_any(|settings| {
            let cur = crate::ue::field::read_f32(settings, self.game_mode_multiplier_offset);
            if cur.is_finite() && cur > 0.0 {
                let _ = self.vanilla_game_mode_multiplier.set(cur);
            }
            if let Some(v) = self.vanilla_game_mode_multiplier.get().copied() {
                crate::ue::field::write_f32(
                    settings,
                    self.game_mode_multiplier_offset,
                    v * (1.0 - reduction),
                );
            }
        });
        let smmc_count = self.mode_manager_components.for_each_instance(|component| {
            let cur = crate::ue::field::read_f32(component, self.mode_manager_multiplier_offset);
            if cur.is_finite() && cur > 0.0 {
                let _ = self.vanilla_mode_manager_multiplier.set(cur);
            }
            if let Some(v) = self.vanilla_mode_manager_multiplier.get().copied() {
                crate::ue::field::write_f32(
                    component,
                    self.mode_manager_multiplier_offset,
                    v * (1.0 - reduction),
                );
            }
        });
        crate::log!(
            "rpg/effects: fall_damage level={} reduction={:.3} written to {} player CDO(s), {} live pawn(s), {} game-mode setting(s), {} mode-manager component(s)",
            level,
            reduction,
            cdo_count,
            live_count,
            gms_count,
            smmc_count
        );
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        format::format_pct(
            0.0,
            self.max_reduction,
            level,
            max_level,
            &PercentFormat::MinusPercent {
                word: "fall damage",
            },
        )
    }
}

/// Scale a survival drain field from its captured vanilla value,
/// the consumer's current setting, and skill progress.
pub struct SurvivalDrainEffect {
    pub class: &'static ClassRef,
    pub field_offset: usize,
    pub vanilla: fn() -> Option<f32>,
    pub settings_multiplier: fn() -> Option<f32>,
    pub max_reduction: f32,
}

impl Effect<UeEngine> for SurvivalDrainEffect {
    fn apply(&self, level: u32, max_level: u32, _ctx: &crate::rpg::TriggerCtx<'_>) {
        let Some(settings_mult) = (self.settings_multiplier)() else {
            return;
        };
        let Some(v) = (self.vanilla)() else {
            return;
        };
        let skill_mult = (1.0 - self.max_reduction * sqrt_progress(level, max_level)).max(0.0);
        let target = v * settings_mult * skill_mult;
        let count = self.class.for_each_cdo_subclass(|obj| {
            crate::ue::field::write_f32(obj, self.field_offset, target);
        });
        crate::log!(
            "rpg/effects: survival_drain level={} target={:.4} (vanilla={:.4} * settings={:.3} * skill={:.3}) written to {} CDO(s)",
            level,
            target,
            v,
            settings_mult,
            skill_mult,
            count
        );
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        let mult = (1.0 - self.max_reduction * sqrt_progress(level, max_level)).max(0.0);
        let pct = ((1.0 - mult) * 100.0).round() as i32;
        format!("-{pct}% drain ({mult:.2}x)")
    }
}

// `RuntimeEffect` moved to `modforge::rpg::std_effect` during
// Phase 0b row 16. Engine-agnostic; blanket `impl<E: Engine>
// Effect<E>`. Re-exported via `crate::rpg::RuntimeEffect`.
pub use modforge::rpg::std_effect::RuntimeEffect;

/// Heal the player for a level-scaled fraction of outgoing
/// player damage by reducing the health component's accumulated
/// damage field.
pub struct LifestealEffect {
    pub player: &'static PlayerRef,
    pub health_component_offset: usize,
    pub current_damage_offset: usize,
    pub max_fraction: f32,
}

impl Effect<UeEngine> for LifestealEffect {
    fn apply(&self, level: u32, max_level: u32, ctx: &crate::rpg::TriggerCtx<'_>) {
        let crate::rpg::TriggerCtx::Engine(crate::rpg::UeEvent::DamageDealt(event)) = ctx else {
            return;
        };
        if !event.attacker_is_player || event.victim_is_player || event.damage <= 0.0 {
            return;
        }
        let progress = sqrt_progress(level, max_level);
        let fraction = self.max_fraction * progress;
        let heal = event.damage * fraction;
        if heal <= 0.0 {
            return;
        }
        // SAFETY: the consumer supplies a player reference and
        // valid offsets for its health-component pointer and f32
        // accumulated-damage field. Damage triggers fire from the
        // game thread inside the DamageHook trampoline.
        unsafe {
            let Some(pawn) = self.player.first_live_static() else {
                return;
            };
            let Some(health_component) =
                crate::ue::field::read_component_ptr(pawn, self.health_component_offset)
            else {
                return;
            };
            let current_damage_field: TypedField<f32> = TypedField::at(self.current_damage_offset);
            let current_damage = current_damage_field.read(health_component);
            if current_damage <= 0.0 {
                return;
            }
            let new_damage = (current_damage - heal).max(0.0);
            current_damage_field.write(health_component, new_damage);
            crate::log!(
                "rpg/lifesteal: dealt {:.2} -> heal {:.2} (level={}, frac={:.3}); CurrentDamage {:.2} -> {:.2}",
                event.damage,
                heal,
                level,
                fraction,
                current_damage,
                new_damage
            );
        }
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        format_pct(
            0.0,
            self.max_fraction,
            level,
            max_level,
            &PercentFormat::PlusPercent { word: "lifesteal" },
        )
    }
}

/// Reverse a level-scaled fraction of matching damage after the
/// engine applies it to the player's health component.
pub struct ImpactReversalEffect {
    pub damage_info: crate::ue::damage_info::DamageInfoLayout,
    pub current_damage_offset: usize,
    pub damage_type_marker: &'static str,
    pub max_reduction: f32,
}

impl Effect<UeEngine> for ImpactReversalEffect {
    fn apply(&self, level: u32, max_level: u32, ctx: &crate::rpg::TriggerCtx<'_>) {
        let crate::rpg::TriggerCtx::Engine(crate::rpg::UeEvent::DamageTaken(event)) = ctx else {
            return;
        };
        if !event.victim_is_player || event.damage <= 0.0 {
            return;
        }
        let damage_type_name = self.damage_info.damage_type_name(event.victim_component);
        if !damage_type_name.contains(self.damage_type_marker) {
            return;
        }
        let progress = sqrt_progress(level, max_level);
        let to_reverse = event.damage * self.max_reduction * progress;
        let current_damage_field: TypedField<f32> = TypedField::at(self.current_damage_offset);
        // SAFETY: event.victim_component is the player health
        // component decoded by DamageHook from the live
        // ProcessEvent call. The consumer supplies a valid f32
        // accumulated-damage offset. DamageTaken fires on the game
        // thread after the engine applies the damage.
        unsafe {
            let current_damage = current_damage_field.read(event.victim_component);
            let new_damage = (current_damage - to_reverse).max(0.0);
            current_damage_field.write(event.victim_component, new_damage);
            crate::log!(
                "rpg/impact: reversed env damage {:.2} (raw={:.2}, level={}); CurrentDamage {:.2} -> {:.2}",
                to_reverse,
                event.damage,
                level,
                current_damage,
                new_damage
            );
        }
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        format_pct(
            0.0,
            self.max_reduction,
            level,
            max_level,
            &PercentFormat::MinusPercent {
                word: "environmental damage",
            },
        )
    }
}

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
        let cur_val =
            unsafe { crate::ue::status_effect::read_row_value(row_ptr, self.status.value_offset) };
        let baseline = self
            .status
            .vanilla
            .get_or_init(self.status.row_fname, cur_val);

        let target = if level > 0 {
            baseline + (self.value_at_max - baseline) * progress
        } else {
            baseline
        };
        // SAFETY: see read above; same row_ptr + offset.
        unsafe {
            crate::ue::status_effect::write_row_value(row_ptr, self.status.value_offset, target);
        }

        let Some(function) = self.component_class.find_function(self.function_name) else {
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
