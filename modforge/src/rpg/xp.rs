//! XP curve math. Cumulative XP for a level + inverse "what level
//! does this XP unlock". Pure math, no IO, no UE.

/// `cumulative_xp_for_level(level) = round(base * level^exponent)`,
/// capped at `max_level`. Defaults match the g2rpg convention:
/// `base = 100`, `exponent = 1.8`, `max_level = 50`. Generous
/// early, flat at the end.
#[derive(Debug, Clone, Copy)]
pub struct Curve {
    pub base: f64,
    pub exponent: f64,
    pub max_level: u32,
}

/// Hard upper bound on `Curve::max_level`. `level_for_xp` is a
/// linear scan; downstream consumers tempted to pick an unbounded
/// max (e.g. "any UE game; pick 10_000") would walk that many
/// iterations on every snapshot. 1024 is plenty for any realistic
/// RPG curve and keeps the scan deterministic.
pub const MAX_LEVEL_LIMIT: u32 = 1024;

impl Curve {
    /// `base = 100`, `exponent = 1.8`, `max_level = 50` is the bbp
    /// default. `max_level` MUST be `<= MAX_LEVEL_LIMIT` (1024); the
    /// `assert!` here makes a 10K-level mistake a build-time failure
    /// when the const is in a `static`, or a panic at runtime
    /// otherwise.
    pub const fn new(base: f64, exponent: f64, max_level: u32) -> Self {
        assert!(
            max_level <= MAX_LEVEL_LIMIT,
            "Curve::new: max_level exceeds MAX_LEVEL_LIMIT"
        );
        Self {
            base,
            exponent,
            max_level,
        }
    }

    /// Cumulative XP required to *reach* `level`. Level 1 = 0.
    pub fn cumulative_xp_for_level(&self, level: u32) -> u64 {
        if level <= 1 {
            return 0;
        }
        let n = level.min(self.max_level) as f64;
        (self.base * n.powf(self.exponent)).round() as u64
    }

    /// What level does `xp` total XP grant? Linear scan. Max_level
    /// is bounded (typical 50-100) so the cost is trivial.
    pub fn level_for_xp(&self, xp: u64) -> u32 {
        for level in (1..=self.max_level).rev() {
            if xp >= self.cumulative_xp_for_level(level) {
                return level;
            }
        }
        1
    }
}

/// Per-creature Def. The workspace-standard `<Subject>Def` for
/// the creature subject. Keyed by BP class short name
/// (`"BP_Aphid_C"`); carries the base XP awarded for killing that
/// creature.
///
/// Future fields (zone scaling, level cap, drop table, AI tags)
/// extend the row without reshaping every consumer.
#[derive(Debug, Clone, Copy)]
pub struct CreatureDef {
    pub class_name: &'static str,
    pub base_xp: u32,
}

/// Per-creature registry. UE5 RPG mods award XP based on what
/// the player killed: a registry keyed by BP class short name
/// mapping to base XP. Wraps a `'static` slice of
/// [`CreatureDef`] rows plus a default-XP fallback for classes
/// not in the table.
///
/// ```ignore
/// pub static CREATURES: CreatureRegistry = CreatureRegistry::new(
///     &[
///         CreatureDef { class_name: "BP_Aphid_C",     base_xp: 5 },
///         CreatureDef { class_name: "BP_Roly_Poly_C", base_xp: 25 },
///         CreatureDef { class_name: "BP_Spider_C",    base_xp: 75 },
///     ],
///     5, // default for unknown classes
/// );
/// // Per-kill convenience:
/// let xp = CREATURES.lookup("BP_Aphid_C"); // 5
/// // Canonical Def lookup:
/// let entry = CREATURES.def("BP_Spider_C"); // Some(&CreatureDef { ... })
/// ```
///
/// Linear scan; realistic registries are <100 rows and lookups
/// are once per kill, so the scan cost is invisible.
#[derive(Debug, Clone, Copy)]
pub struct CreatureRegistry {
    pub entries: &'static [CreatureDef],
    pub default_xp: u32,
}

impl CreatureRegistry {
    pub const fn new(entries: &'static [CreatureDef], default_xp: u32) -> Self {
        Self {
            entries,
            default_xp,
        }
    }

    /// Canonical `<subject>_def` lookup. Returns the full row
    /// or `None` for unknown classes. Callers that just want
    /// the XP value should use [`Self::lookup`].
    pub fn def(&self, class_name: &str) -> Option<&'static CreatureDef> {
        self.entries.iter().find(|e| e.class_name == class_name)
    }

    /// XP for `class_name`. Case-sensitive exact match; falls back
    /// to `default_xp` for unknown classes.
    pub fn lookup(&self, class_name: &str) -> u32 {
        self.def(class_name)
            .map(|e| e.base_xp)
            .unwrap_or(self.default_xp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_one_is_zero() {
        let c = Curve::new(100.0, 1.8, 50);
        assert_eq!(c.cumulative_xp_for_level(1), 0);
        assert_eq!(c.cumulative_xp_for_level(0), 0);
    }

    #[test]
    fn round_trip() {
        let c = Curve::new(100.0, 1.8, 50);
        for level in 1..=50 {
            let xp = c.cumulative_xp_for_level(level);
            assert_eq!(c.level_for_xp(xp), level, "round-trip at level {level}");
        }
    }

    #[test]
    fn caps_at_max_level() {
        let c = Curve::new(100.0, 1.8, 50);
        let xp_max = c.cumulative_xp_for_level(50);
        assert_eq!(c.level_for_xp(xp_max + 999_999), 50);
    }
}
