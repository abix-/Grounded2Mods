//! Engine-agnostic dread loop: the fear of the unknown.
//!
//! A recurring cycle of vague signs followed by delayed payoffs.
//! The sign never reveals the payoff, so the player cannot tell a
//! real threat from a false alarm. Escalation shifts the odds
//! toward real payoffs over time, and a foreshadow chain lets one
//! payoff force the next to be real.
//!
//! Games embed a [`DreadLoop`] and provide their own payoff enum,
//! probability table, sign text, and resolution actions.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use parking_lot::Mutex;

use crate::mission::should_tick;

/// A pseudo-random value in [0, n) from a game-time seed and salt.
/// Deterministic: the same time and salt always produce the same
/// value, so successive rolls in one pass differ by salt alone.
pub fn rng(now: f32, salt: u64, n: u64) -> u64 {
    let mut h = (now.to_bits() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ salt.wrapping_mul(0xD1B5_4A32_D192_ED03);
    h ^= h >> 29;
    h % n.max(1)
}

/// A payoff waiting to land.
pub struct Pending<P> {
    pub payoff: P,
    pub due: f32,
}

/// Shared dread-loop state. Games embed this as a static and call
/// its methods. The framework handles the idle/pending lifecycle,
/// escalation counter, foreshadow flag, and resolve cadence.
pub struct DreadLoop<P> {
    pending: Mutex<Option<Pending<P>>>,
    last_tick: AtomicU32,
    resolved: AtomicU32,
    next_is_real: AtomicBool,
}

impl<P: Copy> DreadLoop<P> {
    pub const fn new() -> Self {
        Self {
            pending: Mutex::new(None),
            last_tick: AtomicU32::new(0),
            resolved: AtomicU32::new(0),
            next_is_real: AtomicBool::new(false),
        }
    }

    /// True while a sign is out and its payoff has not landed.
    pub fn pending(&self) -> bool {
        self.pending.lock().is_some()
    }

    /// How many payoffs have resolved this generation.
    pub fn resolved(&self) -> u32 {
        self.resolved.load(Ordering::Relaxed)
    }

    /// Post a sign with the given payoff. The payoff lands after a
    /// random delay drawn uniformly from [min_delay, max_delay].
    pub fn post(&self, payoff: P, now: f32, min_delay: f32, max_delay: f32) {
        let gap = min_delay
            + (rng(now, 1, 1000) as f32 / 1000.0) * (max_delay - min_delay);
        *self.pending.lock() = Some(Pending {
            payoff,
            due: now + gap,
        });
    }

    /// Check if a pending payoff is due. Returns it and bumps the
    /// escalation counter. Cadence-gated: only checks every
    /// `tick_cadence` seconds.
    pub fn resolve(&self, now: f32, tick_cadence: f32) -> Option<P> {
        if !should_tick(now, tick_cadence, &self.last_tick) {
            return None;
        }
        let ready = {
            let p = self.pending.lock();
            p.as_ref().filter(|p| now >= p.due).map(|p| p.payoff)
        };
        if let Some(payoff) = ready {
            *self.pending.lock() = None;
            self.resolved.fetch_add(1, Ordering::Relaxed);
            Some(payoff)
        } else {
            None
        }
    }

    /// Whether the next payoff should be forced real. Atomically
    /// clears the flag, so call once per sign.
    pub fn take_force_real(&self) -> bool {
        self.next_is_real.swap(false, Ordering::Relaxed)
    }

    /// Set the foreshadow flag: the next sign's payoff will be
    /// forced to a real threat.
    pub fn set_force_real(&self) {
        self.next_is_real.store(true, Ordering::Relaxed);
    }
}
