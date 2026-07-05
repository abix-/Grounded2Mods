//! Survival-driven faction behavior (docs/faction-war.md NORTH
//! STAR: settlements fighting to survive, escalating with
//! desperation).
//!
//! Each AI settlement is continuously assessed on a desperation
//! ladder from its survival state (food, population trend, threat
//! pressure). Behavior escalates with the rung:
//!
//! - Comfortable: grow + defend (development.rs already does the
//!   growth half).
//! - Strained: (future) forage/extort neighbors.
//! - Desperate: RAID A FED NEIGHBOR FOR FOOD. This module ships
//!   this rung: a starving settlement sets an invasion on the
//!   best nearby food-rich community, through the game's own
//!   SetRelationship + SetInvasionTarget (the same machinery
//!   war.rs proved). Vanilla only ever BEGS the player when
//!   hungry; this makes hunger drive real AI-vs-AI war.
//! - Terminal: (future) all-in attack or abandon-and-flee via
//!   StartJourneyToExitMap.
//!
//! Op `survival_status`: every settlement's nutrition, population
//! vs beds vs worldgen size, threat count, and computed
//! desperation rung. The observability surface that makes the
//! living struggle legible.

use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::mono::{self, LogLevel, MonoObject};

use crate::common::{
    base_centre, ctype, display_name, for_each_community, handle_of, list_len, on_main_thread, own,
};
use crate::genome::{self, Trait};

/// Seconds between survival scans (real time). Slower than the
/// growth/recruit ticks: desperation is a slow build, and one
/// hunger-raid per scan keeps the pace organic.
const SURVIVAL_SCAN_PERIOD_SECS: f32 = 90.0;

/// Nutrition at or below this is "desperate" hunger (vanilla's
/// own beg/starve line).
const DESPERATE_NUTRITION: f64 = 0.5;

/// A raid target must have meaningfully MORE food than the
/// raider, so hunger drives them toward the well-fed.
const TARGET_MIN_NUTRITION: f64 = 1.0;

/// A faction must be at least this aggressive to choose raiding
/// over enduring the hunger. Low-aggression camps hold out (and
/// may learn that raiding is not their way, or perish).
const RAID_AGGRESSION_FLOOR: f64 = 0.4;

/// Real seconds after a raid before its outcome is judged (about
/// two survival scans; enough for the squad to reach the target
/// and fight).
const OUTCOME_DELAY_SECS: f32 = 200.0;

/// The ambition war: a COMFORTABLE camp preys on a much weaker
/// neighbor because of who it is, not because it is starving. A
/// voter favors it if their aggression/expansionism blend clears
/// this (a higher bar than hunger's 0.4: unprovoked war takes
/// real appetite).
const AMBITION_FLOOR: f64 = 0.55;

/// Prey must be at most half the predator's size.
const AMBITION_PREY_RATIO: i64 = 2;

/// A predator needs enough people to raid and still hold home.
const AMBITION_MIN_MEMBERS: i64 = 8;

/// Real seconds between ambition wars map-wide: consolidation
/// stays dramatic and paced, not a stampede.
const AMBITION_COOLDOWN_SECS: f32 = 600.0;

/// Suing for peace: a voter wants out of a losing war if their
/// own aggression is at or below this. Meek franchises surrender
/// and live (diminished); proud ones fight on and risk being
/// consumed: selection acting on aggression itself.
const SURRENDER_AGGRESSION_CEILING: f64 = 0.5;

/// A war counts as LOST when the enemy is at least this many
/// times stronger.
const SURRENDER_ODDS: i64 = 2;

/// A pending learning experiment: a raid whose outcome will
/// reinforce or weaken, in every VOTER who chose it, the traits
/// that drove the choice (aggression for hunger raids; aggression
/// + expansionism for ambition wars).
struct Experiment {
    faction_id: i64,
    voter_ids: Vec<i64>,
    traits: Vec<Trait>,
    before_members: i64,
    before_nutrition: f64,
    eval_at: f32,
}

/// The result of a settlement's franchise voting on whether to
/// raid: the collective decision emerges from its enfranchised
/// survivors' individual genomes.
struct Vote {
    franchise: i64,
    for_raid: i64,
    effective_aggression: f64,
    voter_ids: Vec<i64>,
}

/// Tally a settlement's franchise vote. Franchise = who may vote:
/// NORMAL camps enfranchise everyone (fluid identity); LOOTER
/// camps enfranchise only the core (non-conscript) survivors, so
/// the press-ganged are voiceless (stable identity under
/// conquest). Each voter votes yes if `score(their genome)`
/// clears `floor`: the hunger raid scores raw aggression, the
/// ambition war the aggression/expansionism blend.
fn tally_vote(
    com: &MonoObject,
    ctype: &str,
    floor: f64,
    score: impl Fn(&genome::Genome) -> f64,
) -> Result<Vote, String> {
    let looter = ctype == "Looter";
    let mut franchise = 0i64;
    let mut for_raid = 0i64;
    let mut sum_score = 0.0f64;
    let mut voter_ids = Vec::new();

    if let Some(m_h) = handle_of(&com.read_field("Members")?) {
        let mlist = own(m_h);
        let count = mlist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
        for i in 0..count {
            let Some(h) = handle_of(&mlist.invoke("get_Item", &json!([i]))?) else {
                continue;
            };
            let member = own(h);
            let alive = member
                .invoke("get_AliveAndNotZombie", &json!([]))
                .map(|v| v == json!(true))
                .unwrap_or(false);
            let human = member
                .invoke("GetBaseObjectType", &json!([]))
                .map(|v| v == json!("Human"))
                .unwrap_or(false);
            if !alive || !human {
                continue;
            }
            let char_id = member.read_field("Id")?.as_i64().unwrap_or(-1);
            // The franchise rule: Looters exclude conscripts.
            if looter && genome::is_conscript(char_id) {
                continue;
            }
            let g = genome::individual(char_id, ctype);
            let s = score(&g);
            franchise += 1;
            sum_score += s;
            if s >= floor {
                for_raid += 1;
            }
            voter_ids.push(char_id);
        }
    }
    let effective = if franchise > 0 { sum_score / franchise as f64 } else { 0.0 };
    Ok(Vote {
        franchise,
        for_raid,
        effective_aggression: effective,
        voter_ids,
    })
}

static EXPERIMENTS: Mutex<Vec<Experiment>> = Mutex::new(Vec::new());

#[derive(Clone, Copy, PartialEq, Eq)]
enum Rung {
    Comfortable,
    Strained,
    Desperate,
    Terminal,
}

impl Rung {
    fn name(self) -> &'static str {
        match self {
            Rung::Comfortable => "comfortable",
            Rung::Strained => "strained",
            Rung::Desperate => "desperate",
            Rung::Terminal => "terminal",
        }
    }
}

/// A settlement's assessed survival state.
struct Survival {
    nutrition: f64,
    members: i64,
    initial: i64,
    beds: i64,
    threats: i64,
    rung: Rung,
}

fn assess(com: &MonoObject) -> Result<Survival, String> {
    let nutrition = com
        .invoke("CalcCommunityNutritionLevel", &json!([0.0]))?
        .as_f64()
        .unwrap_or(1.0);
    let members = com
        .invoke("GetLivingNonZombieMemberCount", &json!([]))?
        .as_i64()
        .unwrap_or(0);
    let initial = com.read_field("InitialMemberCount")?.as_i64().unwrap_or(members);
    let beds = com.invoke("GetAccommodation", &json!([]))?.as_i64().unwrap_or(0);
    let threats = list_len(com, "Threats");

    // Desperation from the worst survival axis.
    let shrunk = initial > 0 && (members as f64) < (initial as f64) * 0.5;
    let starving = nutrition <= DESPERATE_NUTRITION;
    let tiny = members > 0 && members <= 2;

    let rung = if members == 0 {
        Rung::Terminal
    } else if (starving && (shrunk || tiny)) || (tiny && threats > 0) {
        Rung::Terminal
    } else if starving || shrunk || threats >= 2 {
        Rung::Desperate
    } else if nutrition < TARGET_MIN_NUTRITION || threats >= 1 || (initial > 0 && members < initial) {
        Rung::Strained
    } else {
        Rung::Comfortable
    };

    Ok(Survival {
        nutrition,
        members,
        initial,
        beds,
        threats,
        rung,
    })
}

pub fn register_ops() {
    OP_REGISTRY.register_many([
        OpDef::new(
            "survival_status",
            "Every AI settlement's nutrition, population vs worldgen size vs beds, threat count, computed desperation rung, and its evolving aggression trait. The living-struggle observability surface.",
            "{}",
            survival_status,
        ),
        OpDef::new(
            "genome_status",
            "Every faction's trait genome (aggression/expansionism/defensiveness/guile), keyed by community. Watch personalities diverge as factions learn from their raids.",
            "{}",
            genome_status,
        ),
    ]);
}

fn genome_status(_args: &Json) -> Result<Json, String> {
    // Name each id via a live pass; genomes are held Rust-side.
    let names: std::collections::HashMap<i64, (String, String)> = on_main_thread(|| {
        let mut m = std::collections::HashMap::new();
        for_each_community(|com| {
            let t = ctype(&com);
            if t == "Normal" || t == "Looter" {
                if let Some(id) = com.read_field("Id").ok().and_then(|v| v.as_i64()) {
                    m.insert(id, (display_name(&com), t));
                }
            }
            Ok(true)
        })?;
        Ok(serde_json::to_value(m).unwrap_or(json!({})))
    })
    .ok()
    .and_then(|v| serde_json::from_value(v).ok())
    .unwrap_or_default();

    let mut out = Vec::new();
    for (id, g) in genome::snapshot() {
        let (name, ctype) = names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| ("<gone>".to_string(), "?".to_string()));
        out.push(json!({
            "name": name,
            "type": ctype,
            "traits": g.to_json(),
        }));
    }
    Ok(json!({"factions": out}))
}

// ---- the desperation tick ---------------------------------------------------

static LAST_SCAN_BITS: AtomicU32 = AtomicU32::new(0);

pub fn tick(now: f32) {
    let last = f32::from_bits(LAST_SCAN_BITS.load(Ordering::Relaxed));
    if now - last < SURVIVAL_SCAN_PERIOD_SECS {
        return;
    }
    LAST_SCAN_BITS.store(now.to_bits(), Ordering::Relaxed);
    if let Err(e) = desperation_scan(now) {
        if !e.contains("not found") {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: survival scan failed: {e}"),
            );
        }
    }
}

struct Camp {
    handle: i32,
    id: i64,
    name: String,
    ctype: String,
    nutrition: f64,
    members: i64,
    initial: i64,
    threats: i64,
    rung: Rung,
    centre: Option<(i64, i64)>,
    already_at_war: bool,
    /// The collective's hunger-raid decision (aggression vote).
    voted_to_raid: bool,
    for_raid: i64,
    effective_aggression: f64,
    voter_ids: Vec<i64>,
    /// The collective's ambition-war decision (aggression +
    /// expansionism blend).
    voted_ambition: bool,
    ambition_for: i64,
    effective_ambition: f64,
    ambition_voter_ids: Vec<i64>,
}

fn desperation_scan(now: f32) -> Result<(), String> {
    // First, judge any raids whose outcome is due (the learning
    // half): reinforce/weaken the aggression that drove them.
    evaluate_experiments(now)?;

    // Then resolve any war whose loser is beaten to a husk: the
    // Darwinian selection event (winner consumes loser).
    crate::predation::check_conquests()?;

    // Snapshot every AI settlement once.
    let mut camps: Vec<Camp> = Vec::new();
    for_each_community(|com| {
        let t = ctype(&com);
        if t != "Normal" && t != "Looter" {
            return Ok(true);
        }
        if com.invoke("IsAISettlement", &json!([]))? != json!(true) {
            return Ok(true);
        }
        let sv = assess(&com)?;
        if sv.members == 0 {
            return Ok(true);
        }
        let id = com.read_field("Id")?.as_i64().unwrap_or(-1);
        // The COLLECTIVE decides, not a single faction trait: the
        // enfranchised survivors vote their own genomes. Two
        // ballots per scan: the hunger raid (raw aggression) and
        // the ambition war (aggression/expansionism blend).
        let vote = tally_vote(&com, &t, RAID_AGGRESSION_FLOOR, |g| g.get(Trait::Aggression))?;
        let voted_to_raid = vote.franchise > 0 && vote.for_raid * 2 > vote.franchise;
        let ambition = tally_vote(&com, &t, AMBITION_FLOOR, |g| {
            (g.get(Trait::Aggression) + g.get(Trait::Expansionism)) / 2.0
        })?;
        let voted_ambition = ambition.franchise > 0 && ambition.for_raid * 2 > ambition.franchise;
        let already_at_war = handle_of(&com.read_field("InvasionTarget")?).is_some();
        camps.push(Camp {
            handle: com.handle().0,
            id,
            name: display_name(&com),
            ctype: t,
            nutrition: sv.nutrition,
            members: sv.members,
            initial: sv.initial,
            threats: sv.threats,
            rung: sv.rung,
            centre: base_centre(&com),
            already_at_war,
            voted_to_raid,
            for_raid: vote.for_raid,
            effective_aggression: vote.effective_aggression,
            voter_ids: vote.voter_ids,
            voted_ambition,
            ambition_for: ambition.for_raid,
            effective_ambition: ambition.effective_aggression,
            ambition_voter_ids: ambition.voter_ids,
        });
        std::mem::forget(com); // handles reused across the two passes below
        Ok(true)
    })?;

    // Losing camps may sue for peace before new wars ignite.
    sue_for_peace(&camps)?;

    // ONE war ignition per scan at most; hunger outranks ambition
    // (a starving camp's need beats a fed camp's appetite).
    if !hunger_raid(&camps, now)? {
        ambition_war(&camps, now)?;
    }

    release_camps(&camps);
    Ok(())
}

/// War end: a camp bled below half its worldgen size, facing a
/// hostile at least SURRENDER_ODDS times its strength, holds a
/// peace ballot. A majority of meek voters surrenders through the
/// game's own ceasefire (`SetRelationship(loser, winner,
/// Ceasefire)`: the first argument is recorded as the initiator,
/// which is how the game marks WHO capitulated;
/// `UpdateInvasionTarget` then drops the no-longer-hostile
/// invasion on its own). One surrender per scan.
fn sue_for_peace(camps: &[Camp]) -> Result<bool, String> {
    for loser in camps {
        // Bleeding, but not yet gutted: at 2 or fewer survivors
        // predation decides their fate, not diplomacy.
        let bleeding = loser.members >= 3
            && (loser.members * 2 < loser.initial || loser.rung == Rung::Terminal);
        if !bleeding {
            continue;
        }
        // The strongest enemy they face.
        let mut winner: Option<&Camp> = None;
        for c in camps {
            if c.handle == loser.handle || c.members < loser.members * SURRENDER_ODDS {
                continue;
            }
            let rel = crate::common::with(loser.handle, |l| {
                l.invoke("GetRelationship", &json!([{ "handle": c.handle }]))
            })
            .unwrap_or(json!("?"));
            if rel != json!("Hostile") {
                continue;
            }
            if winner.map(|w| c.members > w.members).unwrap_or(true) {
                winner = Some(c);
            }
        }
        let Some(winner) = winner else { continue };

        // The peace ballot: a voter wants out when their own
        // aggression is at or below the ceiling (score 1-a >= 0.5
        // is exactly a <= 0.5).
        let vote = crate::common::with(loser.handle, |com| {
            tally_vote(com, &loser.ctype, 1.0 - SURRENDER_AGGRESSION_CEILING, |g| {
                1.0 - g.get(Trait::Aggression)
            })
        })?;
        if vote.franchise == 0 || vote.for_raid * 2 <= vote.franchise {
            continue; // too proud; they fight on
        }

        let cm = crate::common::community_manager()?;
        cm.invoke(
            "SetRelationship",
            &json!([{ "handle": loser.handle }, { "handle": winner.handle }, "Ceasefire"]),
        )?;
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: survival -- {} (bled to {} of {}) SURRENDERS to {}: {} of {} voters wanted peace; ceasefire",
                loser.name,
                loser.members,
                loser.initial,
                winner.name,
                vote.for_raid,
                vote.franchise,
            ),
        );
        return Ok(true);
    }
    Ok(false)
}

/// The desperate hunger raid: among desperate, hungry, not-
/// already-warring camps whose COLLECTIVE VOTED to raid, the most
/// bloodthirsty franchise raids the nearest well-fed neighbor. A
/// camp of cautious survivors endures instead. Returns true if a
/// raid ignited.
fn hunger_raid(camps: &[Camp], now: f32) -> Result<bool, String> {
    let raider = camps
        .iter()
        .filter(|c| {
            c.rung == Rung::Desperate
                && c.nutrition <= DESPERATE_NUTRITION
                && !c.already_at_war
                && c.members >= 2
                && c.centre.is_some()
                && c.voted_to_raid
        })
        .max_by(|a, b| {
            a.effective_aggression
                .partial_cmp(&b.effective_aggression)
                .unwrap()
        });
    let Some(raider) = raider else {
        return Ok(false);
    };
    let (rx, ry) = raider.centre.unwrap();

    // Best target: well-fed, not the raider, nearest by base
    // centre.
    let mut best: Option<(&Camp, i64)> = None;
    for c in camps {
        if c.handle == raider.handle || c.nutrition < TARGET_MIN_NUTRITION {
            continue;
        }
        let Some((cx, cy)) = c.centre else { continue };
        let d = (cx - rx) * (cx - rx) + (cy - ry) * (cy - ry);
        if best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((c, d));
        }
    }
    let Some((target, _)) = best else {
        return Ok(false);
    };

    ignite(raider, target, vec![Trait::Aggression], &raider.voter_ids, now)?;
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: survival -- {} ({}, starving at {:.2}) VOTES to raid {} for food: {} of {} voters in favor (effective aggression {:.2})",
            raider.name,
            raider.ctype,
            raider.nutrition,
            target.name,
            raider.for_raid,
            raider.voter_ids.len(),
            raider.effective_aggression,
        ),
    );
    Ok(true)
}

static AMBITION_LAST_BITS: AtomicU32 = AtomicU32::new(0);

/// The ambition war: a comfortable, unthreatened camp whose
/// franchise votes appetite (aggression + expansionism) preys on
/// the nearest neighbor at most half its size. War because of WHO
/// the faction is; with predation downstream, this is the
/// consolidation engine. Paced by a map-wide cooldown.
fn ambition_war(camps: &[Camp], now: f32) -> Result<bool, String> {
    let last = f32::from_bits(AMBITION_LAST_BITS.load(Ordering::Relaxed));
    if last != 0.0 && now - last < AMBITION_COOLDOWN_SECS {
        return Ok(false);
    }
    let predator = camps
        .iter()
        .filter(|c| {
            c.rung == Rung::Comfortable
                && !c.already_at_war
                && c.threats == 0
                && c.members >= AMBITION_MIN_MEMBERS
                && c.centre.is_some()
                && c.voted_ambition
        })
        .max_by(|a, b| {
            a.effective_ambition
                .partial_cmp(&b.effective_ambition)
                .unwrap()
        });
    let Some(predator) = predator else {
        return Ok(false);
    };
    let (px, py) = predator.centre.unwrap();

    // Prey: nearest camp at most half the predator's size that it
    // is not already at war with.
    let mut best: Option<(&Camp, i64)> = None;
    for c in camps {
        if c.handle == predator.handle
            || c.members * AMBITION_PREY_RATIO > predator.members
            || c.members == 0
        {
            continue;
        }
        let Some((cx, cy)) = c.centre else { continue };
        let rel = crate::common::with(predator.handle, |p| {
            p.invoke("GetRelationship", &json!([{ "handle": c.handle }]))
        })
        .unwrap_or(json!("?"));
        if rel == json!("Hostile") || rel == json!("Allied") {
            continue;
        }
        let d = (cx - px) * (cx - px) + (cy - py) * (cy - py);
        if best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((c, d));
        }
    }
    let Some((prey, _)) = best else {
        return Ok(false);
    };

    ignite(
        predator,
        prey,
        vec![Trait::Aggression, Trait::Expansionism],
        &predator.ambition_voter_ids,
        now,
    )?;
    AMBITION_LAST_BITS.store(now.to_bits(), Ordering::Relaxed);
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: survival -- {} ({}, comfortable, {} strong) VOTES to prey on {} ({} members): {} of {} voters hungry for more (effective ambition {:.2})",
            predator.name,
            predator.ctype,
            predator.members,
            prey.name,
            prey.members,
            predator.ambition_for,
            predator.ambition_voter_ids.len(),
            predator.effective_ambition,
        ),
    );
    Ok(true)
}

/// Drive a war through the game's own machinery (same as war.rs's
/// war_ignite): hostile + invasion, and record the learning
/// experiment so the outcome teaches every voter the traits that
/// chose it.
fn ignite(
    raider: &Camp,
    target: &Camp,
    traits: Vec<Trait>,
    voter_ids: &[i64],
    now: f32,
) -> Result<(), String> {
    let raider_obj = own(raider.handle);
    let cm = crate::common::community_manager()?;
    cm.invoke(
        "SetRelationship",
        &json!([{ "handle": raider.handle }, { "handle": target.handle }, "Hostile"]),
    )?;
    raider_obj.invoke(
        "SetInvasionTarget",
        &json!([{ "handle": target.handle }, 7.0, false]),
    )?;
    std::mem::forget(raider_obj);

    EXPERIMENTS.lock().push(Experiment {
        faction_id: raider.id,
        voter_ids: voter_ids.to_vec(),
        traits,
        before_members: raider.members,
        before_nutrition: raider.nutrition,
        eval_at: now + OUTCOME_DELAY_SECS,
    });
    Ok(())
}

/// Judge every raid whose outcome window has closed. A raid that
/// left the faction better off (more people and/or more food)
/// reinforces aggression; one that cost people without gain
/// weakens it. This is the plasticity loop: personality grows
/// from experience.
fn evaluate_experiments(now: f32) -> Result<(), String> {
    let due: Vec<Experiment> = {
        let mut ex = EXPERIMENTS.lock();
        let mut keep = Vec::new();
        let mut fire = Vec::new();
        for e in ex.drain(..) {
            if now >= e.eval_at {
                fire.push(e);
            } else {
                keep.push(e);
            }
        }
        *ex = keep;
        fire
    };
    if due.is_empty() {
        return Ok(());
    }

    // Read each experimenter's current state and reinforce.
    for e in due {
        let mut found: Option<(String, i64, f64)> = None;
        for_each_community(|com| {
            if com.read_field("Id").ok().and_then(|v| v.as_i64()) == Some(e.faction_id) {
                let name = display_name(&com);
                let members = com
                    .invoke("GetLivingNonZombieMemberCount", &json!([]))?
                    .as_i64()
                    .unwrap_or(0);
                let nutrition = com
                    .invoke("CalcCommunityNutritionLevel", &json!([0.0]))?
                    .as_f64()
                    .unwrap_or(0.0);
                found = Some((name, members, nutrition));
                return Ok(false);
            }
            Ok(true)
        })?;
        let Some((name, members_now, nutrition_now)) = found else {
            // The faction died since the raid: the ultimate
            // negative outcome. Its genome dies with it; nothing
            // to reinforce.
            for &t in &e.traits {
                genome::reinforce(e.faction_id, t, false, 2.0);
            }
            continue;
        };

        // Fitness delta: people weigh heavily, food matters too.
        let dmembers = (members_now - e.before_members) as f64;
        let dnutr = nutrition_now - e.before_nutrition;
        let score = dmembers * 1.0 + dnutr * 2.0;
        let magnitude = score.abs().clamp(0.25, 2.0);
        let up = score > 0.15;
        let down = score < -0.15;
        if up || down {
            // Every survivor who VOTED for this raid learns from
            // how it went: the individuals, not the faction,
            // carry the lesson (and it dies or spreads with them),
            // in every trait that drove the choice.
            for &t in &e.traits {
                for &voter in &e.voter_ids {
                    genome::reinforce_individual(voter, t, up, magnitude);
                }
                // Keep the faction-level aggregate roughly in step
                // for the status display.
                genome::reinforce(e.faction_id, t, up, magnitude);
            }
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: learn -- {}'s raid {} (dpop {:+}, dfood {:+.2}); {} voter(s) grow {}",
                    name,
                    if up { "PAID OFF" } else { "COST THEM" },
                    dmembers as i64,
                    dnutr,
                    e.voter_ids.len(),
                    if up { "bolder" } else { "warier" },
                ),
            );
        }
    }
    Ok(())
}

fn release_camps(camps: &[Camp]) {
    for c in camps {
        drop(own(c.handle)); // release the forgotten handles
    }
}

// ---- observability ----------------------------------------------------------

fn survival_status(_args: &Json) -> Result<Json, String> {
    on_main_thread(|| {
        let mut out = Vec::new();
        for_each_community(|com| {
            let t = ctype(&com);
            if t != "Normal" && t != "Looter" {
                return Ok(true);
            }
            if com.invoke("IsAISettlement", &json!([]))? != json!(true) {
                return Ok(true);
            }
            let sv = assess(&com)?;
            if sv.members == 0 {
                return Ok(true);
            }
            let id = com.read_field("Id")?.as_i64().unwrap_or(-1);
            let invasion_target = match handle_of(&com.read_field("InvasionTarget")?) {
                Some(h) => Json::String(display_name(&own(h))),
                None => Json::Null,
            };
            // The collective vote: franchise size, how many favor
            // raiding, and the effective (voted) aggression. In a
            // Looter camp the franchise excludes conscripts, so
            // silenced-vs-total shows the disenfranchised.
            let vote = tally_vote(&com, &t, RAID_AGGRESSION_FLOOR, |g| g.get(Trait::Aggression))?;
            let silenced = sv.members - vote.franchise;
            out.push(json!({
                "name": display_name(&com),
                "type": t,
                "rung": sv.rung.name(),
                "nutrition": (sv.nutrition * 100.0).round() / 100.0,
                "members": sv.members,
                "initial": sv.initial,
                "beds": sv.beds,
                "threats": sv.threats,
                "franchise": vote.franchise,
                "silenced": silenced,
                "votes_to_raid": vote.for_raid,
                "effective_aggression": (vote.effective_aggression * 100.0).round() / 100.0,
                "raiding": invasion_target,
                "stealing": crate::steal::active_target(id).unwrap_or(Json::Null),
                "trading": crate::trade::active_target(id).unwrap_or(Json::Null),
                "robbing": crate::rob::active_target(id).unwrap_or(Json::Null),
            }));
            Ok(true)
        })?;
        // Desperate first, so the struggle is legible at a glance.
        out.sort_by(|a, b| {
            let rank = |v: &Json| match v.get("rung").and_then(Json::as_str) {
                Some("terminal") => 0,
                Some("desperate") => 1,
                Some("strained") => 2,
                _ => 3,
            };
            rank(a).cmp(&rank(b))
        });
        Ok(json!({"settlements": out}))
    })
}
