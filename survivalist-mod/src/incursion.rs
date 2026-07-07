//! The off-map dread loop: the storyteller's anticipation engine
//! (docs/status.md "What a fast-bored player enjoys").
//!
//! The map is a finite box a player eventually solves; the EDGE is
//! the border of an infinite unknown. This loop turns the not-
//! knowing into a system:
//!
//! 1. DREAD. A vague off-map sign posts to the chronicle (smoke on
//!    the horizon, birds gone quiet, refugees who won't say what
//!    they saw). Something is coming, but not what.
//! 2. UNCERTAINTY. A delay lets the dread build. The sign never
//!    reveals the payoff, so the player cannot tell a real threat
//!    from a false alarm.
//! 3. PAYOFF. Sometimes nothing comes (the unease lingers), and
//!    sometimes a real arrival crosses the edge (a stranger of
//!    unknown intent, foreshadowed).
//! 4. ESCALATION. The longer the world turns, the more the signs
//!    pay off and the more ominous they get.
//!
//! v1 pays off on PROVEN ground only: false alarms (chronicle) and
//! stranger arrivals (the strangers system, which now fires ONLY
//! through this loop, so every arrival is foreshadowed). The big
//! threats (mega-horde, off-map raiders, military) plug in here as
//! new payoff kinds once their foundations are verified.

use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;

use unityforge::mono::{self, LogLevel};

use crate::storyteller::{Outcome, Rule};

/// The dread loop as a storyteller rule; the director paces when a
/// new sign appears. Weighted heavier than the other rules: the
/// unknown from beyond is the centerpiece.
pub const RULE: Rule = Rule {
    name: "incursion",
    weight: 2,
    run,
};

/// Seconds between payoff checks.
const RESOLVE_TICK_SECS: f32 = 5.0;

/// The dread window: how long after a sign the payoff lands. Drawn
/// randomly in this band so the wait itself is unpredictable.
const DREAD_MIN_SECS: f32 = 60.0;
const DREAD_MAX_SECS: f32 = 180.0;

#[derive(Clone, Copy)]
enum Payoff {
    FalseAlarm,
    Arrival,
}

struct Pending {
    payoff: Payoff,
    due: f32,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
static LAST_TICK_BITS: AtomicU32 = AtomicU32::new(0);
/// Incursions resolved this generation; drives escalation.
static RESOLVED: AtomicU32 = AtomicU32::new(0);

/// True while a sign is out and its payoff has not landed (for the
/// storyteller status readout).
pub fn pending() -> bool {
    PENDING.lock().is_some()
}

pub fn tick(now: f32) {
    let last = f32::from_bits(LAST_TICK_BITS.load(Ordering::Relaxed));
    if now - last < RESOLVE_TICK_SECS {
        return;
    }
    LAST_TICK_BITS.store(now.to_bits(), Ordering::Relaxed);
    resolve(now);
}

fn run(now: f32) -> Result<Outcome, String> {
    // One sign out at a time: the dread builds, it does not stack.
    if PENDING.lock().is_some() {
        return Ok(Outcome::Passed);
    }
    let resolved = RESOLVED.load(Ordering::Relaxed);
    // Escalation: early on most signs are false alarms; as the story
    // goes on, more of them pay off (60% false down to a 20% floor).
    let false_pct = 60u64.saturating_sub(resolved as u64 * 5).max(20);
    let payoff = if rng(now, 0, 100) < false_pct {
        Payoff::FalseAlarm
    } else {
        Payoff::Arrival
    };
    let gap = DREAD_MIN_SECS
        + (rng(now, 1, 1000) as f32 / 1000.0) * (DREAD_MAX_SECS - DREAD_MIN_SECS);
    *PENDING.lock() = Some(Pending {
        payoff,
        due: now + gap,
    });
    crate::chronicle::post(dread_sign(now, resolved));
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: incursion -- off-map dread sign (payoff {} due in {:.0}s)",
            match payoff {
                Payoff::FalseAlarm => "false-alarm",
                Payoff::Arrival => "arrival",
            },
            gap,
        ),
    );
    Ok(Outcome::Fired)
}

fn resolve(now: f32) {
    let ready = {
        let p = PENDING.lock();
        p.as_ref().filter(|p| now >= p.due).map(|p| p.payoff)
    };
    let Some(payoff) = ready else {
        return;
    };
    *PENDING.lock() = None;
    RESOLVED.fetch_add(1, Ordering::Relaxed);
    match payoff {
        Payoff::FalseAlarm => {
            crate::chronicle::post(false_alarm_line(now));
            mono::log(
                LogLevel::Info,
                "survivalist-mod: incursion -- false alarm; nothing crossed the edge",
            );
        }
        Payoff::Arrival => {
            // The dread had a face: a real group crosses the edge,
            // its intent unknown (the strangers system resolves it).
            if crate::stranger::launch_now(now) {
                crate::chronicle::post("the dread had a face: strangers have crossed onto the map");
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: incursion -- arrival; a stranger band crossed the edge",
                );
            } else {
                // Nobody was close enough to arrive; it passed us by.
                crate::chronicle::post("whatever was out there passed us by, this time");
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: incursion -- arrival rolled, but nothing was near enough to cross",
                );
            }
        }
    }
}

/// A pseudo-random value in [0, n): a hash of the fire time and a
/// salt, so successive rolls differ within one pass.
fn rng(now: f32, salt: u64, n: u64) -> u64 {
    let mut h = (now.to_bits() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ salt.wrapping_mul(0xD1B5_4A32_D192_ED03);
    h ^= h >> 29;
    h % n.max(1)
}

/// The telegraph line. Never hints at the payoff (uncertainty is
/// the point), but grows more ominous as the story escalates.
fn dread_sign(now: f32, resolved: u32) -> &'static str {
    const EARLY: &[&str] = &[
        "smoke rises on the far horizon",
        "the birds have gone quiet to the north",
        "refugees on the road speak of something behind them",
        "a cold wind carries the smell of rot from off the map",
    ];
    const LATE: &[&str] = &[
        "gunfire echoes from beyond the ridge, closer than before",
        "the dead are stirring at the map's edge, drawn by something",
        "travellers will not say what they saw out there",
        "the horizon glows at night where nothing should burn",
    ];
    let pool = if resolved >= 4 { LATE } else { EARLY };
    pool[rng(now, 2, pool.len() as u64) as usize]
}

fn false_alarm_line(now: f32) -> &'static str {
    const L: &[&str] = &[
        "whatever it was, it never came; the unease lingers",
        "the horizon went still again, and no one believes it will last",
        "the threat passed the map by, or is only waiting",
    ];
    L[rng(now, 3, L.len() as u64) as usize]
}
