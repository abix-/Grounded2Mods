//! How much a place gets, and which things it gets.
//!
//! Two mods grew the same shape independently: MISERY's scaling
//! NPC spawner and its alternate-reality overlays. Both roll a
//! budget per map square, both scale it with how far the save has
//! progressed, and both then pick from a weighted list. Neither
//! could be tested, because the logic sat inside a mod that needs
//! the game running.
//!
//! The shape, in plain terms:
//!
//! - a **quiet chance**: sometimes a place gets nothing at all,
//!   whatever the progress level. Without this, every place has
//!   something, and "something everywhere" reads as noise.
//! - a **mean that grows with progress**: `at_zero + per_level *
//!   level`, times an overall `intensity` knob.
//! - a **wide spread around that mean**, so knowing the formula
//!   does not tell you what you will find.
//! - a **cap**, so a high level cannot produce an absurd place.
//!
//! Picking is weighted, with weights that themselves shift with
//! progress, so rare things can become less rare later without
//! ever being guaranteed.

/// A budget: how many things one place gets this time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budget {
    /// Chance the place rolls nothing at all, regardless of
    /// level. 0.0 to 1.0.
    pub quiet_chance: f64,
    /// The mean at level zero.
    pub at_zero: f64,
    /// How much the mean grows per level.
    pub per_level: f64,
    /// One knob over the whole curve. 1.0 leaves it alone.
    pub intensity: f64,
    /// Hard cap on the result.
    pub max: usize,
}

impl Budget {
    /// The average number this place should get at `level`,
    /// before any randomness.
    pub fn mean(&self, level: f64) -> f64 {
        (self.intensity * (self.at_zero + self.per_level * level)).max(0.0)
    }

    /// True when this place gets nothing this time.
    ///
    /// Check this FIRST and return early. A quiet place skips
    /// everything, not just the count: whatever else the caller
    /// rolls (a pack, a bonus, a guaranteed extra) must not
    /// happen either, or "quiet" is not quiet. The roll methods
    /// below deliberately do not check it, so it cannot be
    /// applied twice.
    pub fn is_quiet(&self) -> bool {
        fastrand::f64() < self.quiet_chance
    }

    /// Roll a count proportional to something the place already
    /// has, e.g. "extras compared to the NPCs already here".
    /// Spread is uniform over `0 ..= 2 * mean * scale`, so the
    /// mean is the average and not the outcome. Can return zero.
    ///
    /// Does not check [`is_quiet`]; see its note.
    ///
    /// [`is_quiet`]: Budget::is_quiet
    pub fn roll_scaled(&self, level: f64, scale: f64) -> usize {
        let r = fastrand::f64() * 2.0 * self.mean(level);
        ((scale * r).round().max(0.0) as usize).min(self.max)
    }

    /// Roll an absolute count, always at least one. Uniform over
    /// `1 ..= ceil(2 * mean)`, capped.
    ///
    /// Use this where a place that rolled "something" should
    /// actually get something; use [`roll_scaled`] where zero is
    /// a fine answer. Does not check [`is_quiet`].
    ///
    /// [`roll_scaled`]: Budget::roll_scaled
    /// [`is_quiet`]: Budget::is_quiet
    pub fn roll_count(&self, level: f64) -> usize {
        let span = ((2.0 * self.mean(level)).ceil() as usize).clamp(1, self.max.max(1));
        1 + fastrand::usize(0..span)
    }
}

/// How likely one entry is to be picked, and how that changes as
/// the save progresses.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Weight {
    pub base: f64,
    pub per_level: f64,
}

impl Weight {
    pub const fn new(base: f64, per_level: f64) -> Self {
        Self { base, per_level }
    }

    pub fn at(&self, level: f64) -> f64 {
        (self.base + self.per_level * level).max(0.0)
    }
}

/// Pick one index in proportion to its weight. `None` when every
/// weight is zero.
pub fn pick(weights: &[f64]) -> Option<usize> {
    let total: f64 = weights.iter().filter(|w| **w > 0.0).sum();
    if total <= 0.0 {
        return None;
    }
    let mut roll = fastrand::f64() * total;
    for (i, w) in weights.iter().enumerate() {
        if *w <= 0.0 {
            continue;
        }
        roll -= w;
        if roll <= 0.0 {
            return Some(i);
        }
    }
    // Floating point can leave a sliver; the last positive entry
    // is the honest answer rather than None.
    weights.iter().rposition(|w| *w > 0.0)
}

/// Pick up to `count` DISTINCT indices, weighted at `level`.
///
/// Stops early when nothing is left with a weight above zero, so
/// the result can be shorter than `count`.
pub fn pick_distinct(weights: &[Weight], level: f64, count: usize) -> Vec<usize> {
    let mut live: Vec<f64> = weights.iter().map(|w| w.at(level)).collect();
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        match pick(&live) {
            Some(i) => {
                out.push(i);
                live[i] = 0.0;
            }
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEVER_QUIET: f64 = 0.0;

    fn budget(at_zero: f64, per_level: f64, max: usize) -> Budget {
        Budget {
            quiet_chance: NEVER_QUIET,
            at_zero,
            per_level,
            intensity: 1.0,
            max,
        }
    }

    #[test]
    fn mean_grows_with_level() {
        let b = budget(0.6, 0.03, 4);
        assert_eq!(b.mean(0.0), 0.6);
        assert!((b.mean(10.0) - 0.9).abs() < 1e-9);
        assert!(b.mean(100.0) > b.mean(10.0));
    }

    #[test]
    fn intensity_scales_the_whole_curve() {
        let mut b = budget(1.0, 0.1, 10);
        let plain = b.mean(5.0);
        b.intensity = 2.0;
        assert!((b.mean(5.0) - plain * 2.0).abs() < 1e-9);
    }

    #[test]
    fn mean_never_goes_negative() {
        let b = budget(0.0, -1.0, 10);
        assert_eq!(b.mean(50.0), 0.0);
    }

    #[test]
    fn quiet_chance_of_one_is_always_quiet_and_of_zero_never_is() {
        let always = Budget {
            quiet_chance: 1.0,
            ..budget(10.0, 1.0, 10)
        };
        let never = budget(10.0, 1.0, 10);
        for _ in 0..200 {
            assert!(always.is_quiet());
            assert!(!never.is_quiet());
        }
    }

    #[test]
    fn roll_count_respects_the_cap_and_never_returns_zero_when_loud() {
        let b = budget(100.0, 0.0, 4);
        for _ in 0..500 {
            let n = b.roll_count(0.0);
            assert!((1..=4).contains(&n), "got {n}");
        }
    }

    #[test]
    fn roll_scaled_can_return_zero_and_respects_the_cap() {
        let b = budget(0.0, 1.0, 8);
        let mut saw_zero = false;
        for _ in 0..2000 {
            let n = b.roll_scaled(1.0, 3.0);
            assert!(n <= 8, "got {n}");
            saw_zero |= n == 0;
        }
        assert!(saw_zero, "a wide spread around a small mean should sometimes be zero");
    }

    #[test]
    fn roll_scaled_averages_around_mean_times_scale() {
        // mean 1.0 at level 10, scale 4 -> average near 4.
        let b = budget(0.0, 0.1, 1000);
        let total: usize = (0..20_000).map(|_| b.roll_scaled(10.0, 4.0)).sum();
        let avg = total as f64 / 20_000.0;
        assert!((3.5..4.5).contains(&avg), "average {avg}");
    }

    #[test]
    fn pick_returns_none_when_everything_is_zero() {
        assert_eq!(pick(&[0.0, 0.0, 0.0]), None);
        assert_eq!(pick(&[]), None);
    }

    #[test]
    fn pick_never_returns_a_zero_weight_entry() {
        let weights = [0.0, 5.0, 0.0];
        for _ in 0..500 {
            assert_eq!(pick(&weights), Some(1));
        }
    }

    #[test]
    fn pick_follows_the_weights() {
        let weights = [9.0, 1.0];
        let mut first = 0;
        for _ in 0..10_000 {
            if pick(&weights) == Some(0) {
                first += 1;
            }
        }
        assert!((8_400..9_600).contains(&first), "picked first {first} times");
    }

    #[test]
    fn pick_distinct_never_repeats() {
        let weights = [Weight::new(1.0, 0.0); 5];
        for _ in 0..200 {
            let got = pick_distinct(&weights, 0.0, 5);
            let mut sorted = got.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), got.len(), "repeat in {got:?}");
        }
    }

    #[test]
    fn pick_distinct_stops_when_it_runs_out() {
        let weights = [Weight::new(1.0, 0.0), Weight::new(0.0, 0.0)];
        let got = pick_distinct(&weights, 0.0, 5);
        assert_eq!(got, vec![0]);
    }

    #[test]
    fn weights_shift_with_level() {
        let w = Weight::new(0.0, 0.5);
        assert_eq!(w.at(0.0), 0.0);
        assert_eq!(w.at(4.0), 2.0);
        // A weight that decays cannot go negative and start
        // stealing picks back.
        assert_eq!(Weight::new(1.0, -1.0).at(10.0), 0.0);
    }

    #[test]
    fn a_rare_thing_becomes_less_rare_but_never_certain() {
        let weights = [Weight::new(10.0, 0.0), Weight::new(0.0, 0.2)];
        let count_rare = |level: f64| {
            (0..10_000)
                .filter(|_| pick_distinct(&weights, level, 1) == vec![1])
                .count()
        };
        let early = count_rare(0.0);
        let late = count_rare(40.0);
        assert_eq!(early, 0, "weight zero should never be picked");
        assert!(late > 3_000, "rare thing should be common by level 40: {late}");
        assert!(late < 9_500, "and still not certain: {late}");
    }
}
