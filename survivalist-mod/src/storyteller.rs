//! The storyteller: the director above the base simulation
//! (docs/status.md "Storyteller / director").
//!
//! It paces DRAMA so the apocalypse never goes stale, while holding
//! the brutal-but-survivable line: it never drops a second pressure
//! on a camp already handling too much, so there is always a way
//! out.
//!
//! v1 runs ONE storyteller, RANDY RANDOM: on an irregular random
//! cadence it picks a rule at random and fires it. The horde (the
//! counterweight to the alpha) is its first and only rule; more
//! events register in RULES beside it later. The storyteller is a
//! config field, so Cassandra (rising tension) and Phoebe (slow)
//! can drop in without touching the director. Every knob is
//! tweakable live via the storyteller_config op.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::mono::{self, LogLevel};

use crate::common::{list_len, on_main_thread, session_seed, with};

/// One thing the director can make happen. Kept tiny so a new event
/// (a trader caravan, a wandering band, a plague) registers the
/// same way the horde does: add a Rule to RULES.
pub struct Rule {
    pub name: &'static str,
    /// Relative odds Randy picks this rule.
    pub weight: u32,
    /// Read the world, apply the rule's own preconditions plus (for
    /// pressure events) the survivable guard, and act.
    pub run: fn(now: f32) -> Result<Outcome, String>,
}

/// What a rule did this pass.
pub enum Outcome {
    /// An event happened; the rule posted its own chronicle beat.
    Fired,
    /// Nothing to do (no target, or the survivable guard vetoed).
    /// The director waits a short retry and looks again.
    Passed,
}

/// The registered rules. The horde is rule one; append here.
static RULES: &[Rule] = &[crate::horde::RULE, crate::vendor::RULE, crate::incursion::RULE];

#[derive(Clone, Copy)]
enum Storyteller {
    RandyRandom,
}

/// The director's knobs. Defaults here; tweak live via the
/// storyteller_config op (operator: "lay this out as config").
#[derive(Clone, Copy)]
struct Config {
    storyteller: Storyteller,
    /// Randy draws the wait to the next event uniformly in this band.
    min_gap_secs: f32,
    max_gap_secs: f32,
    /// After a rule finds nothing to do, look again this soon.
    retry_gap_secs: f32,
    /// The survivable line: a camp already tracking more than this
    /// many threats takes no fresh director pressure. 0 spares any
    /// camp already in a fight; raise it if the horde never fires.
    guard_max_threats: i64,
}

const DEFAULT_CONFIG: Config = Config {
    storyteller: Storyteller::RandyRandom,
    min_gap_secs: 180.0,
    max_gap_secs: 600.0,
    retry_gap_secs: 60.0,
    guard_max_threats: 0,
};

static CONFIG: Mutex<Config> = Mutex::new(DEFAULT_CONFIG);

/// When the next event is due, f32 game-seconds in bits. 0 means
/// not scheduled yet; gaps are always > 0 so a real due time never
/// lands on 0.
static NEXT_EVENT_BITS: AtomicU32 = AtomicU32::new(0);

/// The game time of the last tick, for the status readout's
/// seconds-until-next.
static LAST_NOW_BITS: AtomicU32 = AtomicU32::new(0);

/// splitmix64 state; 0 = unseeded. Seeded from the world seed on
/// the first tick with a live game.
static RNG_STATE: AtomicU64 = AtomicU64::new(0);

static LAST_EVENT: Mutex<Option<LastEvent>> = Mutex::new(None);

#[derive(Clone, Copy)]
struct LastEvent {
    rule: &'static str,
    at: f32,
}

pub fn tick(now: f32) {
    if !seeded() {
        return;
    }
    LAST_NOW_BITS.store(now.to_bits(), Ordering::Relaxed);

    let next = NEXT_EVENT_BITS.load(Ordering::Relaxed);
    if next == 0 {
        schedule_next(now, false); // first event a random wait out
        return;
    }
    if now < f32::from_bits(next) {
        return;
    }

    let rule = pick_rule();
    match (rule.run)(now) {
        Ok(Outcome::Fired) => {
            *LAST_EVENT.lock() = Some(LastEvent { rule: rule.name, at: now });
            schedule_next(now, false);
        }
        Ok(Outcome::Passed) => schedule_next(now, true),
        Err(e) => {
            if !e.contains("not found") {
                mono::log(
                    LogLevel::Warn,
                    &format!(
                        "survivalist-mod: storyteller -- rule {} failed: {e}",
                        rule.name
                    ),
                );
            }
            schedule_next(now, true);
        }
    }
}

/// True once the generator is seeded (needs a live game for the
/// world seed).
fn seeded() -> bool {
    if RNG_STATE.load(Ordering::Relaxed) != 0 {
        return true;
    }
    if let Ok(seed) = session_seed() {
        // Mix so a world seed of 0 never leaves us at the unseeded
        // sentinel.
        RNG_STATE.store(((seed as u64) ^ 0xD1B5_4A32_D192_ED03) | 1, Ordering::Relaxed);
        return true;
    }
    false
}

fn schedule_next(now: f32, retry: bool) {
    let cfg = *CONFIG.lock();
    let gap = if retry {
        cfg.retry_gap_secs
    } else {
        let mut s = RNG_STATE.load(Ordering::Relaxed);
        let g = draw_gap(&mut s, cfg.min_gap_secs, cfg.max_gap_secs);
        RNG_STATE.store(s, Ordering::Relaxed);
        g
    };
    NEXT_EVENT_BITS.store((now + gap).to_bits(), Ordering::Relaxed);
}

fn pick_rule() -> &'static Rule {
    let weights: Vec<u32> = RULES.iter().map(|r| r.weight).collect();
    let mut s = RNG_STATE.load(Ordering::Relaxed);
    let i = pick_index(&mut s, &weights);
    RNG_STATE.store(s, Ordering::Relaxed);
    &RULES[i]
}

/// The brutal-but-survivable line for a pressure event: is it safe
/// to lean on this camp? False once it is already handling more
/// threats than the configured ceiling. Pressure rules (the horde)
/// call this before acting.
pub fn safe_to_pressure(community_h: i32) -> bool {
    let max = CONFIG.lock().guard_max_threats;
    with(community_h, |com| list_len(com, "Threats") <= max)
}

// ---- pure helpers (state passed in, so they stay deterministic) ----

fn split_next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Uniform f32 in [0, 1).
fn split_frac(state: &mut u64) -> f32 {
    (split_next(state) >> 40) as f32 / (1u64 << 24) as f32
}

fn draw_gap(state: &mut u64, min: f32, max: f32) -> f32 {
    min + split_frac(state) * (max - min)
}

/// Weighted pick over the rule weights; returns the chosen index.
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

fn storyteller_name(s: Storyteller) -> &'static str {
    match s {
        Storyteller::RandyRandom => "RandyRandom",
    }
}

// ---- ops -----------------------------------------------------------

pub fn register_ops() {
    OP_REGISTRY.register_many([
        OpDef::new(
            "storyteller_status",
            "The director's live state: active storyteller, pacing knobs, seconds until the next event, the last event fired, live director packs, and the current alpha it is watching.",
            "{}",
            storyteller_status,
        ),
        OpDef::new(
            "storyteller_config",
            "Read (no args) or tweak the director's knobs live: min_gap_secs, max_gap_secs, retry_gap_secs, guard_max_threats. Returns the config after any change.",
            "{min_gap_secs?: number, max_gap_secs?: number, retry_gap_secs?: number, guard_max_threats?: number}",
            storyteller_config,
        ),
    ]);
}

fn config_json(cfg: &Config) -> Json {
    json!({
        "storyteller": storyteller_name(cfg.storyteller),
        "min_gap_secs": cfg.min_gap_secs,
        "max_gap_secs": cfg.max_gap_secs,
        "retry_gap_secs": cfg.retry_gap_secs,
        "guard_max_threats": cfg.guard_max_threats,
    })
}

fn storyteller_status(_args: &Json) -> Result<Json, String> {
    on_main_thread(|| {
        let cfg = *CONFIG.lock();
        let next = NEXT_EVENT_BITS.load(Ordering::Relaxed);
        let now = f32::from_bits(LAST_NOW_BITS.load(Ordering::Relaxed));
        let secs_until_next = if next == 0 {
            Json::Null
        } else {
            json!((f32::from_bits(next) - now).max(0.0))
        };
        let last = (*LAST_EVENT.lock()).map(|l| json!({"rule": l.rule, "secs_ago": (now - l.at).max(0.0)}));
        let alpha = crate::horde::alpha_view()?
            .map(|(name, members)| json!({"name": name, "members": members}));
        Ok(json!({
            "config": config_json(&cfg),
            "secs_until_next_event": secs_until_next,
            "packs_live": crate::horde::live_pack_count(),
            "vendors_live": crate::vendor::active_count(),
            "strangers_live": crate::stranger::active_count(),
            "settlers_live": crate::settler::active_count(),
            "incursion_pending": crate::incursion::pending(),
            "last_event": last,
            "alpha": alpha,
        }))
    })
}

fn storyteller_config(args: &Json) -> Result<Json, String> {
    let mut cfg = CONFIG.lock();
    if let Some(v) = args.get("min_gap_secs").and_then(Json::as_f64) {
        cfg.min_gap_secs = v as f32;
    }
    if let Some(v) = args.get("max_gap_secs").and_then(Json::as_f64) {
        cfg.max_gap_secs = v as f32;
    }
    if let Some(v) = args.get("retry_gap_secs").and_then(Json::as_f64) {
        cfg.retry_gap_secs = v as f32;
    }
    if let Some(v) = args.get("guard_max_threats").and_then(Json::as_i64) {
        cfg.guard_max_threats = v;
    }
    // Keep the band well-formed so draw_gap never goes negative.
    if cfg.max_gap_secs < cfg.min_gap_secs {
        cfg.max_gap_secs = cfg.min_gap_secs;
    }
    Ok(config_json(&cfg))
}
