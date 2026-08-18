//! `StandardEffect`. The canonical UE5 RPG skill-effect enum.
//!
//! The vast majority of UE5 RPG-mod skills fall into one of nine
//! shapes. ueforge ships them all as variants of a single enum
//! with built-in apply + format dispatch. Game crates that pick
//! from this menu need ZERO custom apply code:
//!
//! ```ignore
//! pub static CATALOG: &[Skill<StandardEffect>] = &[
//!     Skill {
//!         id: "attack_damage",
//!         display_name: "Attack Damage",
//!         max_level: 100,
//!         effect: StandardEffect::PlayerFloat {
//!             offset: TypedField::at(0x12B8),
//!             base: 1.0,
//!             max_bonus: 3.0,
//!             format: PercentFormat::PlusPercentMult { word: "damage" },
//!         },
//!     },
//!     // ...
//! ];
//!
//! impl RpgApplier for GameApplier {
//!     type Effect = StandardEffect;
//!     fn apply_skill(&self, state: &SkillsState, skill: &Skill<StandardEffect>) {
//!         skill.effect.apply(state.level_of(skill.id), skill.max_level, &PLAYER);
//!     }
//!     fn format_effect(&self, skill: &Skill<StandardEffect>, level: u32) -> String {
//!         skill.effect.format(level, skill.max_level)
//!     }
//! }
//! ```
//!
//! Games that need shapes outside the menu (composite writes,
//! game-specific UFunction calls, etc.) define their own enum
//! that wraps `StandardEffect`:
//!
//! ```ignore
//! pub enum SkillEffect {
//!     Standard(StandardEffect),
//!     CustomBackpack { /* G2-specific InventoryComponent CDO write */ },
//!     CustomFallReduction { /* multi-component composite */ },
//! }
//! ```
//!
//! and dispatch in their applier:
//!
//! ```ignore
//! match skill.effect {
//!     SkillEffect::Standard(e) => e.apply(level, max, player),
//!     SkillEffect::CustomBackpack { .. } => apply_backpack(level, ...),
//!     SkillEffect::CustomFallReduction { .. } => apply_fall(level, ...),
//! }
//! ```
//!
//! The 90% case is fully covered by `StandardEffect`; the 10% of
//! game-specific composites stays a one-off in the game crate.
//!
//! ## Design rules
//!
//! - **All variant fields are `'static`.** Variants live in the
//!   `static CATALOG` so their fields must outlive the program.
//!   `&'static ClassRef`, `&'static PlayerRef`, `&'static
//!   VanillaCache<...>`, `TypedField<T>` (Copy), `&'static [usize]`,
//!   `fn() -> ...`. No closures, no `Vec`, no `String`.
//! - **Every variant carries its own `VanillaCache` reference**
//!   when it needs vanilla baselines. Caches are static; tests +
//!   debug surfaces can introspect them.
//! - **Apply runs on the game thread** (called from
//!   `Tracker::activate_slot` / `spend` / `refund`, which the
//!   game crate funnels through). Variants do CDO walks +
//!   live-pawn walks, all of which read GObjects.
//! - **Hot-path safety isn't a concern here.** Apply is cold
//!   path; trampolines that need per-fire skill levels read them
//!   from `Tracker::with_state` directly via a `Runtime` variant.

use crate::rpg::format::{self, PercentFormat, format_pct};
use crate::rpg::progress::sqrt_progress;
use crate::rpg::vanilla::VanillaCache;
use crate::ue::{ClassRef, PlayerRef, TypedField, UObject};

/// The canonical UE5 RPG skill-effect variant menu. See module
/// docs for the variant-by-variant guide and the "wrap if you
/// need extras" pattern.
#[derive(Clone, Copy)]
pub enum StandardEffect {
    /// Direct `f32` write at a fixed offset on the player class
    /// (CDO + live pawns). Final value: `base + max_bonus *
    /// progress`.
    ///
    /// Used for fields directly on the pawn class.
    /// `ASurvivalCharacter.CustomDamageMultiplier` etc.
    PlayerFloat {
        offset: TypedField<f32>,
        base: f32,
        max_bonus: f32,
        format: PercentFormat,
    },

    /// `f32` write on a player **subcomponent** (HealthComponent,
    /// CharMovementComponent, etc.) reached by following a
    /// `*mut UObject` pointer at `component_offset` from the
    /// pawn. Final value: `base + max_bonus * progress`.
    PlayerSubcomponentFloat {
        component_offset: TypedField<*mut UObject>,
        field_offset: TypedField<f32>,
        base: f32,
        max_bonus: f32,
        format: PercentFormat,
    },

    /// Additive `f32` on a subcomponent: capture vanilla on
    /// first sight, write `vanilla + max_bonus * progress`.
    /// Used for stacking-style fields (Max Health) where we
    /// want to ADD to whatever the engine has, not REPLACE.
    PlayerSubcomponentAdditive {
        component_offset: TypedField<*mut UObject>,
        field_offset: TypedField<f32>,
        max_bonus: f32,
        format_word: &'static str,
        /// Single-baseline cache keyed by the field offset.
        vanilla: &'static VanillaCache<usize, f32>,
    },

    /// `u32` mask write on a subcomponent. When `level > 0`,
    /// write `mask`; when `level == 0`, restore the captured
    /// vanilla. Useful for binary-gate fields like
    /// `RequiredDamageTypeFlags`.
    PlayerSubcomponentU32Mask {
        component_offset: TypedField<*mut UObject>,
        field_offset: TypedField<u32>,
        mask: u32,
        format: PercentFormat,
        vanilla: &'static VanillaCache<usize, u32>,
    },

    /// Multiply each of `offsets` on a player subcomponent by
    /// `1 + max_bonus * progress`, scaled per-axis from the
    /// captured vanilla. Used for movement skills that scale
    /// multiple CharMovementComponent fields together.
    PlayerSubcomponentMultiply {
        component_offset: TypedField<*mut UObject>,
        offsets: &'static [usize],
        max_bonus: f32,
        format_word: &'static str,
        vanilla: &'static VanillaCache<usize, f32>,
    },

    /// Multiply offsets on every instance (CDO + non-CDO) of a
    /// class. `(offset, exponent)` pairs let "boost" fields use
    /// `+1.0` and "shrink" fields (regen tick rate, etc.) use
    /// `-1.0`. Final value at offset = `vanilla * (1 + max_bonus
    /// * progress)^exponent`.
    ClassFieldsMultiply {
        class: &'static ClassRef,
        /// `(field_offset, exponent)` pairs.
        offsets: &'static [(usize, f32)],
        max_bonus: f32,
        format_word: &'static str,
        vanilla: &'static VanillaCache<usize, f32>,
    },

    /// No CDO write. The skill's effect is implemented entirely
    /// in a hot-path callback (kill_hook trampoline reads the
    /// level on every fire). The variant exists to formalize
    /// "this is a Runtime skill" in the catalog so the format
    /// helper can render the right text.
    Runtime {
        max_bonus: f32,
        format: PercentFormat,
    },

    /// Apply a UE5 row-driven status effect to the player's
    /// `UStatusEffectComponent`. Mutates the row's `Value` field
    /// to `vanilla + (value_at_max - vanilla) * progress`, then
    /// invokes the `function_name` UFunction (typically
    /// `CreateAndAddEffect`) on the player's component with a
    /// `(table, row_fname)` row-handle parm.
    ///
    /// The `table_finder` closure resolves the data table once
    /// per apply. Cache the result statically inside the
    /// closure if needed.
    StatusEffect {
        /// Resolves the `UDataTable` carrying the row.
        table_finder: fn() -> Option<&'static UObject>,
        /// `&UClass` of the player's status-effect component
        /// (e.g. "StatusEffectComponent" or
        /// "GearStatusEffectComponent" depending on game).
        component_class: &'static ClassRef,
        /// `*mut UObject` at this offset on the player IS the
        /// component instance.
        component_offset: TypedField<*mut UObject>,
        /// UFunction name on `component_class` to call.
        /// Typically `"CreateAndAddEffect"`.
        function_name: &'static str,
        /// FName of the row to mutate + apply.
        row_fname: u64,
        /// Row's `Value` field byte offset within the row
        /// struct. Defaults to the Maine layout
        /// (`crate::ue::status_effect::DEFAULT_VALUE_OFFSET`,
        /// `0x34`); override per game.
        value_offset: usize,
        /// Target value at level == max_level. At level == 0
        /// the captured vanilla is restored.
        value_at_max: f32,
        format_word: &'static str,
        /// Per-row vanilla cache keyed by row_fname.
        vanilla: &'static VanillaCache<u64, f32>,
    },
}

impl StandardEffect {
    /// Apply this skill at `level` against the player. Cold path
    ///. Called on slot activate / spend / refund / toggle. Each
    /// variant walks GObjects + writes targets as appropriate.
    pub fn apply(&self, level: u32, max_level: u32, player: &PlayerRef) {
        let progress = sqrt_progress(level, max_level);
        match self {
            StandardEffect::PlayerFloat {
                offset, base, max_bonus, ..
            } => {
                apply_player_float(player, *offset, *base, *max_bonus, progress);
            }
            StandardEffect::PlayerSubcomponentFloat {
                component_offset, field_offset, base, max_bonus, ..
            } => {
                apply_subcomponent_float(
                    player, *component_offset, *field_offset,
                    *base, *max_bonus, progress,
                );
            }
            StandardEffect::PlayerSubcomponentAdditive {
                component_offset, field_offset, max_bonus, vanilla, ..
            } => {
                apply_subcomponent_additive(
                    player, *component_offset, *field_offset,
                    *max_bonus, progress, vanilla,
                );
            }
            StandardEffect::PlayerSubcomponentU32Mask {
                component_offset, field_offset, mask, vanilla, ..
            } => {
                apply_subcomponent_u32_mask(
                    player, *component_offset, *field_offset,
                    *mask, level, vanilla,
                );
            }
            StandardEffect::PlayerSubcomponentMultiply {
                component_offset, offsets, max_bonus, vanilla, ..
            } => {
                apply_subcomponent_multiply(
                    player, *component_offset, offsets,
                    *max_bonus, progress, vanilla,
                );
            }
            StandardEffect::ClassFieldsMultiply {
                class, offsets, max_bonus, vanilla, ..
            } => {
                apply_class_fields_multiply(
                    class, offsets, *max_bonus, progress, vanilla,
                );
            }
            StandardEffect::Runtime { .. } => {
                // No-op. The hot-path callback owns the effect.
            }
            StandardEffect::StatusEffect {
                table_finder, component_class, component_offset,
                function_name, row_fname, value_offset, value_at_max,
                vanilla, ..
            } => {
                apply_status_effect(
                    player, table_finder, component_class,
                    *component_offset, function_name,
                    *row_fname, *value_offset, *value_at_max,
                    level, progress, vanilla,
                );
            }
        }
    }

    /// Format the at-level effect text for the ImGui tab.
    pub fn format(&self, level: u32, max_level: u32) -> String {
        match self {
            StandardEffect::PlayerFloat { base, max_bonus, format, .. }
            | StandardEffect::PlayerSubcomponentFloat { base, max_bonus, format, .. } => {
                format_pct(*base, *max_bonus, level, max_level, format)
            }
            StandardEffect::PlayerSubcomponentAdditive { max_bonus, format_word, .. } => {
                format::format_additive_f32_as_int(*max_bonus, level, max_level, format_word)
            }
            StandardEffect::PlayerSubcomponentU32Mask { format, .. } => {
                // Boolean-style: at any non-zero level the gate is
                // fully active. Pass max_bonus=1.0 to surface the
                // intent in the format string.
                let bonus = if level > 0 { 1.0 } else { 0.0 };
                format_pct(0.0, bonus, level, max_level, format)
            }
            StandardEffect::PlayerSubcomponentMultiply { max_bonus, format_word, .. }
            | StandardEffect::ClassFieldsMultiply { max_bonus, format_word, .. } => {
                format::format_multiplier(*max_bonus, level, max_level, format_word)
            }
            StandardEffect::Runtime { max_bonus, format } => {
                format_pct(0.0, *max_bonus, level, max_level, format)
            }
            StandardEffect::StatusEffect { value_at_max, format_word, .. } => {
                let progress = sqrt_progress(level, max_level);
                let pct = (progress * 100.0).round() as i32;
                let target = value_at_max * progress;
                format!("{pct}% {format_word} (to value={target:.2})")
            }
        }
    }
}

// ---------------------------------------------------------------
// Per-variant apply helpers. All cold path.
// ---------------------------------------------------------------

fn apply_player_float(
    player: &PlayerRef,
    offset: TypedField<f32>,
    base: f32,
    max_bonus: f32,
    progress: f32,
) -> usize {
    let value = base + max_bonus * progress;
    let mut count = 0;
    player.for_each_cdo(|cdo| {
        unsafe { offset.write(cdo, value) };
        count += 1;
    });
    player.for_each_live(|p| {
        unsafe { offset.write(p, value) };
        count += 1;
    });
    count
}

fn apply_subcomponent_float(
    player: &PlayerRef,
    comp_offset: TypedField<*mut UObject>,
    field_offset: TypedField<f32>,
    base: f32,
    max_bonus: f32,
    progress: f32,
) -> usize {
    let value = base + max_bonus * progress;
    let mut count = 0;
    let mut walk = |obj: &UObject| unsafe {
        if let Some(comp) = comp_offset.deref(obj) {
            field_offset.write(comp, value);
            count += 1;
        }
    };
    player.for_each_cdo(&mut walk);
    player.for_each_live(&mut walk);
    count
}

fn apply_subcomponent_additive(
    player: &PlayerRef,
    comp_offset: TypedField<*mut UObject>,
    field_offset: TypedField<f32>,
    max_bonus: f32,
    progress: f32,
    vanilla: &VanillaCache<usize, f32>,
) -> usize {
    let mut count = 0;
    let off_key = field_offset.offset();
    let mut walk = |obj: &UObject| unsafe {
        if let Some(comp) = comp_offset.deref(obj) {
            let cur = field_offset.read(comp);
            // Capture vanilla on first sight; ignore non-finite.
            let baseline = if cur.is_finite() && cur != 0.0 {
                vanilla.get_or_init(off_key, cur)
            } else {
                vanilla.get(off_key).unwrap_or(cur)
            };
            field_offset.write(comp, baseline + max_bonus * progress);
            count += 1;
        }
    };
    player.for_each_cdo(&mut walk);
    player.for_each_live(&mut walk);
    count
}

fn apply_subcomponent_u32_mask(
    player: &PlayerRef,
    comp_offset: TypedField<*mut UObject>,
    field_offset: TypedField<u32>,
    mask: u32,
    level: u32,
    vanilla: &VanillaCache<usize, u32>,
) -> usize {
    let mut count = 0;
    let off_key = field_offset.offset();
    let mut walk = |obj: &UObject| unsafe {
        if let Some(comp) = comp_offset.deref(obj) {
            let cur = field_offset.read(comp);
            // Capture vanilla on first sight (whatever is there
            // before we touch it). Subsequent reads return the
            // stored baseline.
            vanilla.get_or_init(off_key, cur);
            let target = if level > 0 {
                mask
            } else {
                vanilla.get(off_key).unwrap_or(cur)
            };
            field_offset.write(comp, target);
            count += 1;
        }
    };
    player.for_each_cdo(&mut walk);
    player.for_each_live(&mut walk);
    count
}

fn apply_subcomponent_multiply(
    player: &PlayerRef,
    comp_offset: TypedField<*mut UObject>,
    offsets: &[usize],
    max_bonus: f32,
    progress: f32,
    vanilla: &VanillaCache<usize, f32>,
) -> usize {
    let mult = 1.0 + max_bonus * progress;
    let mut count = 0;
    let mut walk = |obj: &UObject| unsafe {
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
            count += 1;
        }
    };
    player.for_each_cdo(&mut walk);
    player.for_each_live(&mut walk);
    count
}

fn apply_class_fields_multiply(
    class: &ClassRef,
    offsets: &[(usize, f32)],
    max_bonus: f32,
    progress: f32,
    vanilla: &VanillaCache<usize, f32>,
) -> usize {
    let mult = 1.0 + max_bonus * progress;
    let mut count = 0;
    class.for_each_any(|obj| unsafe {
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
        count += 1;
    });
    count
}

#[allow(clippy::too_many_arguments)]
fn apply_status_effect(
    player: &PlayerRef,
    table_finder: &fn() -> Option<&'static UObject>,
    component_class: &ClassRef,
    component_offset: TypedField<*mut UObject>,
    function_name: &str,
    row_fname: u64,
    value_offset: usize,
    value_at_max: f32,
    level: u32,
    progress: f32,
    vanilla: &VanillaCache<u64, f32>,
) -> usize {
    let Some(table) = table_finder() else {
        crate::log!("rpg/std_effect: status-effect: table not loaded yet");
        return 0;
    };
    let row_fname_handle = crate::ue::FName::from_u64(row_fname);
    let Some(row_ptr) = (unsafe {
        crate::ue::datatable::row_value_by_fname(table, row_fname_handle)
    }) else {
        crate::log!(
            "rpg/std_effect: status-effect: row 0x{:016x} not found in table",
            row_fname
        );
        return 0;
    };

    // Capture vanilla once.
    let cur_val = unsafe { crate::ue::status_effect::read_row_value(row_ptr, value_offset) };
    let baseline = vanilla.get_or_init(row_fname, cur_val);

    let target = if level > 0 {
        baseline + (value_at_max - baseline) * progress
    } else {
        baseline
    };
    unsafe {
        crate::ue::status_effect::write_row_value(row_ptr, value_offset, target);
    }

    // Resolve the UFunction on the component class once per apply.
    let Some(function) = component_class.find_function(function_name) else {
        crate::log!(
            "rpg/std_effect: status-effect: {} not found on {}",
            function_name,
            component_class.name()
        );
        return 0;
    };

    let mut count = 0;
    player.for_each_live(|pawn| {
        if let Some(component) = unsafe { component_offset.deref(pawn) } {
            crate::ue::status_effect::create_and_add_effect(
                component, function, table, row_fname,
            );
            count += 1;
        }
    });
    count
}

// Re-export for convenience. Consumers usually want both
// PercentFormat and StandardEffect from the same prelude.
pub use crate::rpg::format::PercentFormat as StdPercentFormat;
