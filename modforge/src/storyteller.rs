//! Storyteller / director: tick-driven event pacer for game mods.
//!
//! The director picks a weighted-random rule on an irregular
//! cadence, fires it, and schedules the next event. Config knobs
//! are tweakable live via the standard `storyteller_config` op.
//!
//! Engine-agnostic. Games supply their own rules and seed source.
//!
//! ```ignore
//! use modforge::storyteller::{Director, Rule, Outcome};
//!
//! static RULES: &[Rule] = &[
//!     Rule { name: "horde", weight: 1, run: horde_run },
//!     Rule { name: "vendor", weight: 1, run: vendor_run },
//! ];
//!
//! static DIRECTOR: Director = Director::new(RULES);
//!
//! // in your frame tick:
//! DIRECTOR.tick(now, || session_seed());
//! ```

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use crate::ops::{OP_REGISTRY, OpDef};

/// One thing the director can make happen.
pub struct Rule {
    pub name: &'static str,
    pub weight: u32,
    pub run: fn(now: f32) -> Result<Outcome, String>,
}

/// What a rule did this pass.
pub enum Outcome {
    Fired,
    Passed,
}

/// One caller-observed target for adaptive pressure. The caller
/// supplies engine facts; Modforge owns eligibility, threshold,
/// and strongest-target selection.
pub struct PressureTarget<T> {
    pub eligible: bool,
    pub pressure: i64,
    pub value: T,
}

/// Select the first strongest eligible target at or above the
/// minimum pressure. Equal-pressure ties preserve caller order.
pub fn strongest_pressure_target<T>(
    targets: impl IntoIterator<Item = PressureTarget<T>>,
    minimum_pressure: i64,
) -> Option<PressureTarget<T>> {
    let mut strongest: Option<PressureTarget<T>> = None;
    for target in targets {
        if !target.eligible || target.pressure < minimum_pressure {
            continue;
        }
        if strongest
            .as_ref()
            .map(|current| target.pressure > current.pressure)
            .unwrap_or(true)
        {
            strongest = Some(target);
        }
    }
    strongest
}

/// One caller-defined adaptive-pressure tier.
pub struct PressureTier<T> {
    pub at_least: i64,
    pub value: T,
}

/// Resolve the first tier whose threshold the pressure reaches.
/// Callers order tiers from strongest to weakest.
pub fn pressure_tier<T>(pressure: i64, tiers: &[PressureTier<T>]) -> Option<&PressureTier<T>> {
    tiers.iter().find(|tier| pressure >= tier.at_least)
}

/// Pick a deterministic point on a ring around a pressure target.
pub fn pressure_ring_position(now: f32, salt: u64, centre: (i64, i64), radius: f64) -> (i64, i64) {
    let mut hash = (now.to_bits() as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (salt << 17);
    hash ^= hash >> 29;
    let angle = (hash % 6283) as f64 / 1000.0;
    (
        centre.0 + (angle.cos() * radius) as i64,
        centre.1 + (angle.sin() * radius) as i64,
    )
}

struct ActivePressure<I, H> {
    target_id: I,
    handle: H,
}

/// Active adaptive-pressure lifecycle: global cap, per-target
/// exclusion, tracking, and caller-driven liveness pruning.
pub struct PressureTracker<I, H> {
    max_active: usize,
    active: Mutex<Vec<ActivePressure<I, H>>>,
}

impl<I, H> PressureTracker<I, H>
where
    I: Copy + Eq,
    H: Copy,
{
    pub const fn new(max_active: usize) -> Self {
        Self {
            max_active,
            active: Mutex::new(Vec::new()),
        }
    }

    /// Remove finished pressure events. The caller observes engine
    /// liveness and releases engine resources for each removed handle.
    pub fn prune(&self, mut is_alive: impl FnMut(H) -> bool, mut cleanup: impl FnMut(H)) {
        self.active.lock().retain(|event| {
            let alive = is_alive(event.handle);
            if !alive {
                cleanup(event.handle);
            }
            alive
        });
    }

    pub fn is_full(&self) -> bool {
        self.active.lock().len() >= self.max_active
    }

    pub fn is_targeted(&self, target_id: I) -> bool {
        self.active
            .lock()
            .iter()
            .any(|event| event.target_id == target_id)
    }

    pub fn track(&self, target_id: I, handle: H) {
        self.active
            .lock()
            .push(ActivePressure { target_id, handle });
    }

    pub fn len(&self) -> usize {
        self.active.lock().len()
    }
}

/// Pacing knobs. Defaults can be overridden at construction or
/// live via the `storyteller_config` op.
#[derive(Clone, Copy)]
pub struct Config {
    pub min_gap_secs: f32,
    pub max_gap_secs: f32,
    pub retry_gap_secs: f32,
}

impl Config {
    pub const fn default_config() -> Self {
        Self {
            min_gap_secs: 180.0,
            max_gap_secs: 600.0,
            retry_gap_secs: 60.0,
        }
    }

    pub fn to_json(&self) -> Json {
        json!({
            "min_gap_secs": self.min_gap_secs,
            "max_gap_secs": self.max_gap_secs,
            "retry_gap_secs": self.retry_gap_secs,
        })
    }

    pub fn apply_args(&mut self, args: &Json) {
        if let Some(v) = args.get("min_gap_secs").and_then(Json::as_f64) {
            self.min_gap_secs = v as f32;
        }
        if let Some(v) = args.get("max_gap_secs").and_then(Json::as_f64) {
            self.max_gap_secs = v as f32;
        }
        if let Some(v) = args.get("retry_gap_secs").and_then(Json::as_f64) {
            self.retry_gap_secs = v as f32;
        }
        if self.max_gap_secs < self.min_gap_secs {
            self.max_gap_secs = self.min_gap_secs;
        }
    }
}

#[derive(Clone, Copy)]
struct LastEvent {
    rule: &'static str,
    at: f32,
}

pub struct Director {
    rules: &'static [Rule],
    config: Mutex<Config>,
    next_event_bits: AtomicU32,
    last_now_bits: AtomicU32,
    rng_state: AtomicU64,
    last_event: Mutex<Option<LastEvent>>,
}

impl Director {
    pub const fn new(rules: &'static [Rule]) -> Self {
        Self {
            rules,
            config: Mutex::new(Config::default_config()),
            next_event_bits: AtomicU32::new(0),
            last_now_bits: AtomicU32::new(0),
            rng_state: AtomicU64::new(0),
            last_event: Mutex::new(None),
        }
    }

    pub const fn with_config(rules: &'static [Rule], config: Config) -> Self {
        Self {
            rules,
            config: Mutex::new(config),
            next_event_bits: AtomicU32::new(0),
            last_now_bits: AtomicU32::new(0),
            rng_state: AtomicU64::new(0),
            last_event: Mutex::new(None),
        }
    }

    /// Drive the director forward. Call once per frame tick.
    ///
    /// `seed_fn` returns the world seed for RNG seeding; called
    /// only until the first successful seed. Return `Err` when
    /// no game is loaded yet.
    pub fn tick(
        &self,
        now: f32,
        seed_fn: impl FnOnce() -> Result<i64, String>,
        on_error: impl FnOnce(&str, &str),
    ) {
        if !self.ensure_seeded(seed_fn) {
            return;
        }
        self.last_now_bits.store(now.to_bits(), Ordering::Relaxed);

        let next = self.next_event_bits.load(Ordering::Relaxed);
        if next == 0 {
            self.schedule_next(now, false);
            return;
        }
        if now < f32::from_bits(next) {
            return;
        }

        let rule = self.pick_rule();
        match (rule.run)(now) {
            Ok(Outcome::Fired) => {
                *self.last_event.lock() = Some(LastEvent { rule: rule.name, at: now });
                self.schedule_next(now, false);
            }
            Ok(Outcome::Passed) => self.schedule_next(now, true),
            Err(e) => {
                if !e.contains("not found") {
                    on_error(rule.name, &e);
                }
                self.schedule_next(now, true);
            }
        }
    }

    /// Snapshot of the director's core state for status ops.
    pub fn status(&self) -> Json {
        let cfg = *self.config.lock();
        let next = self.next_event_bits.load(Ordering::Relaxed);
        let now = f32::from_bits(self.last_now_bits.load(Ordering::Relaxed));
        let secs_until_next = if next == 0 {
            Json::Null
        } else {
            json!((f32::from_bits(next) - now).max(0.0))
        };
        let last = (*self.last_event.lock())
            .map(|l| json!({"rule": l.rule, "secs_ago": (now - l.at).max(0.0)}));
        json!({
            "config": cfg.to_json(),
            "secs_until_next_event": secs_until_next,
            "last_event": last,
        })
    }

    /// Read or tweak config. Returns the config after any change.
    pub fn apply_config(&self, args: &Json) -> Json {
        let mut cfg = self.config.lock();
        cfg.apply_args(args);
        cfg.to_json()
    }

    /// Register the standard `storyteller_config` op. Games
    /// register their own `storyteller_status` since it includes
    /// game-specific fields.
    pub fn register_config_op(&'static self) {
        OP_REGISTRY.register(OpDef::new(
            "storyteller_config",
            "Read (no args) or tweak the director's knobs live: min_gap_secs, max_gap_secs, retry_gap_secs.",
            "{min_gap_secs?: number, max_gap_secs?: number, retry_gap_secs?: number}",
            move |args| Ok(self.apply_config(args)),
        ));
    }

    fn ensure_seeded(&self, seed_fn: impl FnOnce() -> Result<i64, String>) -> bool {
        if self.rng_state.load(Ordering::Relaxed) != 0 {
            return true;
        }
        if let Ok(seed) = seed_fn() {
            self.rng_state
                .store(((seed as u64) ^ 0xD1B5_4A32_D192_ED03) | 1, Ordering::Relaxed);
            return true;
        }
        false
    }

    fn schedule_next(&self, now: f32, retry: bool) {
        let cfg = *self.config.lock();
        let gap = if retry {
            cfg.retry_gap_secs
        } else {
            let mut s = self.rng_state.load(Ordering::Relaxed);
            let g = draw_gap(&mut s, cfg.min_gap_secs, cfg.max_gap_secs);
            self.rng_state.store(s, Ordering::Relaxed);
            g
        };
        self.next_event_bits
            .store((now + gap).to_bits(), Ordering::Relaxed);
    }

    fn pick_rule(&self) -> &'static Rule {
        let weights: Vec<u32> = self.rules.iter().map(|r| r.weight).collect();
        let mut s = self.rng_state.load(Ordering::Relaxed);
        let i = pick_index(&mut s, &weights);
        self.rng_state.store(s, Ordering::Relaxed);
        &self.rules[i]
    }
}

// ---- pure RNG helpers ------------------------------------------------

fn split_next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn split_frac(state: &mut u64) -> f32 {
    (split_next(state) >> 40) as f32 / (1u64 << 24) as f32
}

fn draw_gap(state: &mut u64, min: f32, max: f32) -> f32 {
    min + split_frac(state) * (max - min)
}

fn pick_index(state: &mut u64, weights: &[u32]) -> usize {
    let total: u64 = weights.iter().map(|&w| u64::from(w)).sum();
    if total == 0 {
        return 0;
    }
    let roll = split_next(state) % total;
    let mut acc = 0u64;
    for (i, &w) in weights.iter().enumerate() {
        acc += u64::from(w);
        if roll < acc {
            return i;
        }
    }
    0
}
