//! Engine-agnostic quality tier system: roll a tier from
//! cumulative per-mille odds, pick a statistical sibling.
//!
//! Quality tiers are ranked item variants (Legendary, Epic, Rare,
//! Uncommon above the base Normal). Each tier has several
//! statistical siblings with jittered stats sharing one display
//! name, so two Rare rifles are usually not exactly the same.
//!
//! Games define their tier names, odds tables (per sender), and
//! swap mechanics. This module provides the probability rolls.

use crate::unknown::rng;

/// Roll a tier index from cumulative per-mille odds (best tier
/// first). Returns None for the base/common tier. Each entry in
/// `odds` is the per-mille chance of that tier (1 = 0.1%, 10 = 1%),
/// evaluated cumulatively from the top.
pub fn roll_tier(odds: &[u64], now: f32, salt: u64) -> Option<usize> {
    let r = rng(now, salt, 1000);
    let mut cum = 0u64;
    for (i, &o) in odds.iter().enumerate() {
        cum += o;
        if r < cum {
            return Some(i);
        }
    }
    None
}

/// Pick a sibling number in [1, siblings]. Each tier has several
/// statistical siblings with jittered stats; this picks which one.
pub fn roll_sibling(siblings: u64, now: f32, salt: u64) -> u64 {
    rng(now, salt.wrapping_mul(97), siblings) + 1
}
