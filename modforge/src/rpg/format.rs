//! Skill-effect text formatting.
//!
//! Every UE RPG mod ends up writing the same six format shapes
//! for the ImGui tab + log lines: "+25% damage", "-50% drain",
//! "+30% lifesteal", "+460 slots", "+200 max health". Lift the
//! enum + the format function once.
//!
//! ```ignore
//! use ueforge::rpg::format::{PercentFormat, format_pct};
//!
//! let line = format_pct(
//!     /* base */ 1.0,
//!     /* max_bonus */ 3.0,
//!     /* level */ 50,
//!     /* max_level */ 100,
//!     &PercentFormat::PlusPercentMult { word: "damage" },
//! );
//! assert_eq!(line, "+212% damage (3.12x)"); // sqrt(50/100)=~0.707
//! ```

use super::progress::sqrt_progress;

/// Tag for the four idiomatic percent-format shapes.
#[derive(Debug, Clone, Copy)]
pub enum PercentFormat {
    /// "+25% damage (1.25x)". Multiplier-style buff.
    PlusPercentMult { word: &'static str },
    /// "-25% damage taken". Reduction-style buff.
    MinusPercent { word: &'static str },
    /// "+30% lifesteal". Raw fraction, no multiplier shown.
    PlusPercent { word: &'static str },
    /// "-25% drain (0.75x)". Multiplicative-drain reduction.
    DrainReduction,
}

/// Compute the post-progress bonus for a `(level, max_level,
/// max_bonus)` tuple using the canonical `sqrt(level/max_level)`
/// curve.
///
/// Use this any time you'd otherwise write
/// `max_bonus * sqrt_progress(level, max_level)`.
pub fn skill_bonus(max_bonus: f32, level: u32, max_level: u32) -> f32 {
    max_bonus * sqrt_progress(level, max_level)
}

/// Format the at-level effect as "+N% word (Xx)" / "-N% word" /
/// "+N% word" / drain text per the chosen shape.
///
/// `base` is the engine's vanilla value (e.g. 1.0 for damage
/// multiplier). The formatter shows `base + bonus` as the
/// resulting multiplier where applicable.
pub fn format_pct(
    base: f32,
    max_bonus: f32,
    level: u32,
    max_level: u32,
    fmt: &PercentFormat,
) -> String {
    let bonus = skill_bonus(max_bonus, level, max_level);
    let value = base + bonus;
    match fmt {
        PercentFormat::PlusPercentMult { word } => {
            let pct = (bonus * 100.0).round() as i32;
            format!("+{pct}% {word} ({value:.2}x)")
        }
        PercentFormat::MinusPercent { word } => {
            let pct = (bonus * 100.0).round() as i32;
            format!("-{pct}% {word}")
        }
        PercentFormat::PlusPercent { word } => {
            let pct = (bonus * 100.0).round() as i32;
            format!("+{pct}% {word}")
        }
        PercentFormat::DrainReduction => {
            let mult = (1.0 - bonus).max(0.0);
            let pct = (bonus * 100.0).round() as i32;
            format!("-{pct}% drain ({mult:.2}x)")
        }
    }
}

/// "+N% word (Xx)". Multiplier-style write for skills whose
/// effective multiplier is `1 + max_bonus * progress` (the shape
/// `PlayerMovementMult` and `GlobalDataMult` use).
pub fn format_multiplier(max_bonus: f32, level: u32, max_level: u32, word: &str) -> String {
    let bonus = skill_bonus(max_bonus, level, max_level);
    let mult = 1.0 + bonus;
    let pct = (bonus * 100.0).round() as i32;
    format!("+{pct}% {word} ({mult:.2}x)")
}

/// "+N word". Raw additive count for things like "+460 slots".
pub fn format_additive_int(max_bonus: i32, level: u32, max_level: u32, word: &str) -> String {
    let bonus = (max_bonus as f32 * sqrt_progress(level, max_level)).round() as i32;
    format!("+{bonus} {word}")
}

/// "+N word". Additive float rendered as int. Used for things
/// like "+200 max health" where the underlying value is f32 but
/// the user-visible figure is an integer count.
pub fn format_additive_f32_as_int(
    max_bonus: f32,
    level: u32,
    max_level: u32,
    word: &str,
) -> String {
    let bonus = skill_bonus(max_bonus, level, max_level).round() as i32;
    format!("+{bonus} {word}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_bonus_zero_at_zero() {
        assert_eq!(skill_bonus(3.0, 0, 100), 0.0);
    }

    #[test]
    fn skill_bonus_full_at_max() {
        assert!((skill_bonus(3.0, 100, 100) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn plus_percent_mult_at_max() {
        let s = format_pct(
            1.0,
            3.0,
            100,
            100,
            &PercentFormat::PlusPercentMult { word: "damage" },
        );
        assert_eq!(s, "+300% damage (4.00x)");
    }

    #[test]
    fn minus_percent_at_quarter() {
        // sqrt(25/100) = 0.5, so 50% bonus
        let s = format_pct(
            0.0,
            1.0,
            25,
            100,
            &PercentFormat::MinusPercent {
                word: "damage taken",
            },
        );
        assert_eq!(s, "-50% damage taken");
    }

    #[test]
    fn drain_reduction_at_max() {
        let s = format_pct(0.0, 1.0, 100, 100, &PercentFormat::DrainReduction);
        assert_eq!(s, "-100% drain (0.00x)");
    }

    #[test]
    fn additive_int_rounds() {
        // sqrt(25/100) = 0.5
        assert_eq!(format_additive_int(460, 25, 100, "slots"), "+230 slots");
    }
}
