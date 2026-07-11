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
//! Payoff kinds: false alarms (chronicle), stranger arrivals (the
//! strangers system, which now fires ONLY through this loop, so
//! every arrival is foreshadowed), off-map raiders (forced hostile),
//! military remnants (hostile to everything, purpose never
//! explained), a settling faction (a real group claims a dead base
//! via the game's own reclamation), the mysterious stranger (a lone
//! figure whose meaning is never learned), a refugee wave (real
//! groups seek shelter and what they fled arrives on the next
//! sign), an off-map signal (a broadcast luring toward the edge),
//! and the traveling mega-horde.
//! The hostile payoffs rest on foundations not yet verified live (a
//! group actually attacking; the horde spawner needs the game
//! restart).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::json;

use unityforge::mono::{self, LogLevel};

use crate::common::{
    base_centre, community_manager, ctype, display_name, for_each_community, handle_of, own, with,
};
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
    Raiders,
    Military,
    Settlers,
    MysteriousStranger,
    RefugeeWave,
    Signal,
    MegaHorde,
}

struct Pending {
    payoff: Payoff,
    due: f32,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
static LAST_TICK_BITS: AtomicU32 = AtomicU32::new(0);
/// Incursions resolved this generation; drives escalation.
static RESOLVED: AtomicU32 = AtomicU32::new(0);
/// Set when a refugee wave lands: what they fled FOLLOWS them, so
/// the next sign's payoff is forced to a real threat.
static NEXT_IS_REAL: AtomicBool = AtomicBool::new(false);

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
    let payoff = if NEXT_IS_REAL.swap(false, Ordering::Relaxed) {
        // A refugee wave landed: what they fled arrives next, never
        // a false alarm and never harmless.
        if resolved >= 6 && rng(now, 6, 100) < 30 {
            Payoff::MegaHorde
        } else if resolved >= 4 && rng(now, 7, 100) < 40 {
            Payoff::Military
        } else {
            Payoff::Raiders
        }
    } else if rng(now, 0, 100) < false_pct {
        Payoff::FalseAlarm
    } else if resolved >= 6 && rng(now, 6, 100) < 15 {
        // Rare and late: the tide of the dead from beyond the map.
        Payoff::MegaHorde
    } else if resolved >= 4 && rng(now, 7, 100) < 20 {
        // Late and rare: a military remnant crossing on a mission,
        // hostile to everything it passes, purpose never explained.
        Payoff::Military
    } else if resolved >= 2 && rng(now, 8, 100) < 15 {
        // An off-map offshoot comes to STAY: a new camp on a dead
        // base rewrites the balance, so the map is never solved.
        Payoff::Settlers
    } else if rng(now, 9, 100) < 15 {
        // Any time at all: a lone figure whose meaning is never
        // learned. The lingering mystery is the point.
        Payoff::MysteriousStranger
    } else if rng(now, 10, 100) < 12 {
        // A wave of real survivors running from something beyond
        // the edge; what they fled comes for the map next.
        Payoff::RefugeeWave
    } else if rng(now, 11, 100) < 12 {
        // A broadcast from beyond the edge: a lure toward the
        // unknown, never explained, never located.
        Payoff::Signal
    } else {
        // Among real payoffs, raiders grow likelier as it escalates:
        // early arrivals are mostly benign, late ones mean to fight.
        let raiders_pct = (30u64 + resolved as u64 * 5).min(70);
        if rng(now, 4, 100) < raiders_pct {
            Payoff::Raiders
        } else {
            Payoff::Arrival
        }
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
                Payoff::Raiders => "raiders",
                Payoff::Military => "military",
                Payoff::Settlers => "settlers",
                Payoff::MysteriousStranger => "mysterious-stranger",
                Payoff::RefugeeWave => "refugee-wave",
                Payoff::Signal => "signal",
                Payoff::MegaHorde => "mega-horde",
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
        Payoff::Raiders => match spawn_edge_band(now, "HunterLooter", false) {
            Ok(Some(camp)) => {
                crate::chronicle::post(&format!(
                    "raiders have crossed onto the map, and they mean to take what {camp} has"
                ));
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: incursion -- RAIDERS; a hostile band spawned at the edge and marches on {camp}"
                    ),
                );
            }
            Ok(None) => {
                crate::chronicle::post("something stirred at the edge, but found nowhere to go");
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: incursion -- raiders rolled, but there was nowhere to cross toward",
                );
            }
            Err(e) if e.contains("pre-v6") => {
                crate::chronicle::post("armed men gather beyond the ridge, but do not come; not yet");
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: incursion -- raiders held back (a game restart arms the spawner)",
                );
            }
            Err(e) => {
                mono::log(
                    LogLevel::Warn,
                    &format!("survivalist-mod: incursion -- raiders failed: {e}"),
                );
            }
        },
        Payoff::Military => match spawn_edge_band(now, "HunterMercenary", true) {
            Ok(Some(_)) => {
                crate::chronicle::post(
                    "soldiers have crossed onto the map, and they are not here to help anyone",
                );
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: incursion -- MILITARY; a remnant unit spawned at the edge, hostile to every camp",
                );
            }
            Ok(None) => {
                crate::chronicle::post(
                    "the soldiers passed beyond the edge; no one learned their mission",
                );
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: incursion -- military rolled, but there was nowhere to cross toward",
                );
            }
            Err(e) if e.contains("pre-v6") => {
                crate::chronicle::post("something moves in formation past the ridge, but does not come; not yet");
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: incursion -- military held back (a game restart arms the spawner)",
                );
            }
            Err(e) => {
                mono::log(
                    LogLevel::Warn,
                    &format!("survivalist-mod: incursion -- military failed: {e}"),
                );
            }
        },
        Payoff::Settlers => {
            // An off-map offshoot walks in to claim a dead base and
            // stays: the map's balance rewritten from beyond.
            if crate::settler::launch_now(now) {
                crate::chronicle::post(
                    "a band has crossed the edge looking for a place to stay, and found dead walls to claim",
                );
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: incursion -- SETTLERS; an off-map faction crossed to claim a dead base",
                );
            } else {
                crate::chronicle::post(
                    "wanderers passed along the edge, but nothing out there stayed",
                );
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: incursion -- settlers rolled, but no group and claimable base matched",
                );
            }
        }
        Payoff::MysteriousStranger => {
            // A lone figure crosses the edge; whatever they do at
            // the gate, the meaning is never explained.
            if crate::stranger::launch_mysterious(now) {
                crate::chronicle::post(
                    "someone crossed the edge alone; no one knows what they want",
                );
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: incursion -- MYSTERIOUS STRANGER; a lone figure crossed the edge",
                );
            } else {
                crate::chronicle::post(
                    "the watchers swore someone stood at the map's edge; there was no one",
                );
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: incursion -- mysterious stranger rolled, but no lone figure was near",
                );
            }
        }
        Payoff::RefugeeWave => {
            // Real groups cross and seek shelter; the wave arms the
            // foreshadow so what they fled arrives on the NEXT sign.
            let n = crate::stranger::launch_refugees(now);
            if n > 0 {
                NEXT_IS_REAL.store(true, Ordering::Relaxed);
                crate::chronicle::post(
                    "refugees are crossing onto the map, fleeing something they will not name",
                );
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: incursion -- REFUGEE WAVE; {n} group(s) crossed, fleeing something off-map"
                    ),
                );
            } else {
                crate::chronicle::post(
                    "a trickle of refugees passed the map by; whatever drives them is far away, for now",
                );
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: incursion -- refugee wave rolled, but no groups were near enough to cross",
                );
            }
        }
        Payoff::Signal => {
            // Pure lure: the broadcast is heard and never explained.
            // Whether anyone walks toward it is their own choice.
            crate::chronicle::post(&signal_line(now));
            mono::log(
                LogLevel::Info,
                "survivalist-mod: incursion -- SIGNAL; a broadcast from beyond the edge, never explained",
            );
        }
        Payoff::MegaHorde => match mega_horde(now) {
            Ok(true) => {
                crate::chronicle::post(
                    "a tide of the dead is crossing the map, and nothing in its path will stand",
                );
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: incursion -- MEGA-HORDE; a tide of the dead crosses the map",
                );
            }
            Ok(false) => {
                crate::chronicle::post("the horizon holds, for now");
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: incursion -- mega-horde rolled, but there was nowhere to cross toward",
                );
            }
            Err(e) if e.contains("pre-v6") => {
                crate::chronicle::post("something vast stirs at the edge, but does not come; not yet");
                mono::log(
                    LogLevel::Info,
                    "survivalist-mod: incursion -- mega-horde held back (a game restart arms the spawner)",
                );
            }
            Err(e) => {
                mono::log(
                    LogLevel::Warn,
                    &format!("survivalist-mod: incursion -- mega-horde failed: {e}"),
                );
            }
        },
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

/// The centroid of all settlements and the spread (max distance
/// from the centroid to any of them): the populated heart of the
/// map and roughly how wide it is, in tile coordinates. None when
/// there is nothing to threaten. Scale-independent, so it works
/// without knowing the map's absolute bounds.
fn map_centroid_and_spread() -> Result<Option<((i64, i64), i64)>, String> {
    let mut sum = (0i64, 0i64);
    let mut centres: Vec<(i64, i64)> = Vec::new();
    for_each_community(|com| {
        let t = ctype(&com);
        if (t == "Normal" || t == "Looter" || t == "Player")
            && let Some(c) = base_centre(&com)
        {
            sum.0 += c.0;
            sum.1 += c.1;
            centres.push(c);
        }
        Ok(true)
    })?;
    if centres.is_empty() {
        return Ok(None);
    }
    let n = centres.len() as i64;
    let centroid = (sum.0 / n, sum.1 / n);
    let spread = centres
        .iter()
        .map(|c| {
            let (dx, dy) = (c.0 - centroid.0, c.1 - centroid.1);
            (((dx * dx + dy * dy) as f64).sqrt()) as i64
        })
        .max()
        .unwrap_or(0)
        .max(200);
    Ok(Some((centroid, spread)))
}

/// The traveling mega-horde: a large pack of the worst strain spawns
/// beyond every camp in a random direction and walks THROUGH the
/// populated heart of the map, crossing whatever is in its path.
/// Reuses the horde's spawner. Returns whether it spawned; errs
/// "pre-v6" until a game restart arms the spawner.
fn mega_horde(now: f32) -> Result<bool, String> {
    let Some((centroid, spread)) = map_centroid_and_spread()? else {
        return Ok(false);
    };
    let angle = rng(now, 5, 6283) as f64 / 1000.0;
    let radius = spread as f64 * 1.6 + 200.0;
    let sx = centroid.0 + (angle.cos() * radius) as i64;
    let sy = centroid.1 + (angle.sin() * radius) as i64;
    let pointed = crate::horde::spawn_traveling_pack(sx, sy, centroid, 16, 24, "White")?;
    Ok(pointed > 0)
}

/// The off-map generator (docs/status.md design lock): spawn a REAL
/// band at the map edge from the undefined beyond. The game's own
/// `SpawnAmbientLooters` creates a band of `kind` carrying real gear
/// and generated loot, so EVERY off-map arrival (hostile or not)
/// feeds fresh loot and people into the world. Returns (band handle,
/// band id, spawn tile), or None if there is no populated map to
/// anchor an edge to, or the spawn produced nobody (an impassable
/// edge point). `salt` varies the edge direction so several bands in
/// one pass land apart. `armed` gives raider-grade weapons; light
/// otherwise. Errs "pre-v6" until a game restart arms static invoke,
/// like the horde. The caller owns the returned band handle.
pub fn spawn_band_at_edge(
    now: f32,
    salt: u64,
    kind: &str,
    min_count: i64,
    max_count: i64,
    armed: bool,
) -> Result<Option<(i32, i64, (i64, i64))>, String> {
    let Some((centroid, spread)) = map_centroid_and_spread()? else {
        return Ok(None);
    };
    let angle = rng(now, salt, 6283) as f64 / 1000.0;
    let radius = spread as f64 * 1.4 + 150.0;
    let ex = centroid.0 + (angle.cos() * radius) as i64;
    let ey = centroid.1 + (angle.sin() * radius) as i64;
    let (melee, ammo, molotov, body, helmet, leg) = if armed {
        (0.85, 0.6, 0.3, 0.35, 0.3, 0.3)
    } else {
        (0.3, 0.1, 0.0, 0.1, 0.05, 0.05)
    };
    let band = mono::invoke_static(
        "LooterSpawnPoint",
        "SpawnAmbientLooters",
        &json!([
            {"x": ex, "y": ey}, min_count, max_count,
            melee, ammo, molotov, body, helmet, leg, null, kind
        ]),
    )?;
    let Some(band_h) = handle_of(&band) else {
        return Ok(None); // impassable edge point / nobody spawned
    };
    let id = with(band_h, |b| b.read_field("Id").ok().and_then(|v| v.as_i64())).unwrap_or(-1);
    Ok(Some((band_h, id, (ex, ey))))
}

/// Raiders/military: spawn an armed band at the edge and set it
/// hostile (all camps for a military remnant, the nearest for
/// raiders); the game's combat AI marches them in. Returns the
/// target camp's name.
fn spawn_edge_band(now: f32, kind: &str, hostile_all: bool) -> Result<Option<String>, String> {
    let Some((band_h, _id, (ex, ey))) = spawn_band_at_edge(now, 20, kind, 3, 6, true)? else {
        return Ok(None);
    };

    // The edge rolls quality: a military remnant may carry a fine
    // rifle in place of a common one (quality.rs first slice).
    if hostile_all {
        crate::quality::upgrade_band_gear(band_h, now);
    }

    // The camps, with handles, to point the band at.
    let mut camps: Vec<(i32, (i64, i64), String)> = Vec::new();
    for_each_community(|com| {
        let t = ctype(&com);
        if (t == "Normal" || t == "Looter" || t == "Player")
            && com.invoke("GetLivingNonZombieMemberCount", &json!([]))?.as_i64().unwrap_or(0) > 0
            && let Some(c) = base_centre(&com)
        {
            camps.push((com.handle().0, c, display_name(&com)));
            std::mem::forget(com);
        }
        Ok(true)
    })?;
    if camps.is_empty() {
        drop(own(band_h));
        return Ok(None);
    }

    // Nearest camp to where the band crossed.
    let mut best = 0usize;
    let mut best_d = i64::MAX;
    for (i, (_, c, _)) in camps.iter().enumerate() {
        let d = (c.0 - ex).pow(2) + (c.1 - ey).pow(2);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }

    // Set the band hostile (all camps for a military unit, just the
    // nearest for raiders) and point its invasion at the target; the
    // game's own combat AI marches them from here.
    let cm = community_manager()?;
    for (i, (ch, _, _)) in camps.iter().enumerate() {
        if hostile_all || i == best {
            let _ = cm.invoke(
                "SetRelationship",
                &json!([{ "handle": band_h }, { "handle": *ch }, "Hostile"]),
            );
        }
    }
    let target_h = camps[best].0;
    let target_name = camps[best].2.clone();
    let _ = with(band_h, |g| {
        g.invoke("SetInvasionTarget", &json!([{ "handle": target_h }, 7.0, false]))
    });

    drop(own(band_h));
    for (ch, _, _) in camps {
        drop(own(ch));
    }
    Ok(Some(target_name))
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

/// The off-map signal: a broadcast luring listeners toward the
/// edge. It names a direction (the lure needs somewhere to point)
/// but never a meaning.
fn signal_line(now: f32) -> String {
    const DIRS: &[&str] = &["north", "south", "east", "west"];
    const L: &[&str] = &[
        "a radio crackles to life: a voice beyond the {} edge repeats the same words, then static",
        "for one night a broadcast from the {} promises safety to any who come; by morning it is gone",
        "a signal from beyond the {} edge counts numbers into the dark, over and over",
    ];
    let dir = DIRS[rng(now, 12, DIRS.len() as u64) as usize];
    L[rng(now, 13, L.len() as u64) as usize].replace("{}", dir)
}

fn false_alarm_line(now: f32) -> &'static str {
    const L: &[&str] = &[
        "whatever it was, it never came; the unease lingers",
        "the horizon went still again, and no one believes it will last",
        "the threat passed the map by, or is only waiting",
    ];
    L[rng(now, 3, L.len() as u64) as usize]
}
