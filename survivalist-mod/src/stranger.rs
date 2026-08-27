//! Strangers of unknown intent: a storyteller event, the wildcard
//! kind (docs/status.md "Storyteller / director"; the event menu).
//!
//! Real survivors walk onto the map from the edge (the game's own
//! roving-refugee inflow, the same groups growth.rs recruits).
//! When the director fires this, it CLAIMS one of those real groups,
//! rolls a HIDDEN intent toward the nearest camp or the player, and
//! announces only that strangers are approaching. The truth is
//! revealed when they reach the gate, and it has real shades so the
//! unknown never goes stale:
//!
//! - FRIENDLY: they came in peace, and either JOIN the camp (real
//!   recruitment) or, as passing traders, SHARE some of their own
//!   supplies and move on.
//! - AGGRESSIVE: they came for blood; the group is set hostile and
//!   the game's own combat AI takes over.
//! - WARY: they size up the camp and move on, or SHAKE IT DOWN for
//!   a stack of tribute first.
//!
//! Nothing is conjured: the group is real, the intent roll is fair
//! because a stranger has no prior relationship to fabricate, and
//! every outcome runs through the game's own machinery (real goods
//! move by the same Take/Add transfer everything else uses). The
//! not-knowing is the point; growth.rs skips a claimed group so
//! this system owns its fate.

use std::sync::atomic::AtomicU32;

use parking_lot::Mutex;
use serde_json::json;

use modforge::mission::{self, OneStageStep};
use unityforge::mono::{self, LogLevel, MonoObject};

use crate::common::{
    GoodsFilter, base_centre, carry_off_stored_goods, community_manager, ctype, display_name,
    for_each_community, handle_of, march_band_to, own, pos_of, with,
};
use crate::storyteller::Outcome;

/// Force-launch a stranger now with a ROLLED hidden intent,
/// reporting whether a group was near enough to actually cross. The
/// incursion loop drives this, so every stranger arrives foreshadowed
/// by off-map dread.
/// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
pub fn launch_now(now: f32) -> bool {
    matches!(launch_with(now, None), Ok(Outcome::Fired))
}

/// Force-launch the mysterious stranger: a LONE figure (a one-member
/// group) whose meaning is never learned. Same arrival machinery;
/// the reveal is real but never explained.
/// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
pub fn launch_mysterious(now: f32) -> bool {
    matches!(
        launch_with(now, Some(Intent::Mysterious)),
        Ok(Outcome::Fired)
    )
}

/// Force-launch a refugee wave: up to WAVE_MAX real groups steered
/// to camps as shelter-seekers who will not say what they fled.
/// Returns how many groups crossed. The incursion loop drives this
/// and follows it with a real threat: the wave is the foreshadow.
/// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
pub fn launch_refugees(now: f32) -> usize {
    let mut launched = 0;
    for _ in 0..WAVE_MAX {
        if !matches!(launch_with(now, Some(Intent::Refugee)), Ok(Outcome::Fired)) {
            break;
        }
        launched += 1;
    }
    launched
}

/// Seconds between resolve passes (arrival checks).
const MISSION_TICK_SECS: f32 = 5.0;

/// The band has reached the gate within this range: reveal the
/// intent.
const RESOLVE_RANGE: f32 = 48.0;

/// A band that has not reached its camp by then wandered off.
const MISSION_TIMEOUT_SECS: f32 = 900.0;

/// At most this many bands in play map-wide.
const MAX_STRANGERS: usize = 2;

/// At most this many refugee-wave groups in play map-wide (a wave
/// has its own cap so it can cross while a band is already out).
const WAVE_MAX: usize = 3;

/// Non-food stacks a friendly trader shares / a shakedown takes.
const SHARE_STACKS: i64 = 2;
const TRIBUTE_STACKS: i64 = 1;

/// The rolled outcome, hidden until the band reaches the gate. The
/// unknown is the fear, so aggression is a real chance.
#[derive(Clone, Copy)]
enum Intent {
    FriendlyJoin,
    FriendlyShare,
    Aggressive,
    Mysterious,
    Refugee,
    WaryLeave,
    Shakedown,
}

struct Mission {
    group_h: i32,
    group_id: i64,
    target_h: i32,
    target_name: String,
    intent: Intent,
    deadline: f32,
}

static MISSIONS: Mutex<Vec<Mission>> = Mutex::new(Vec::new());
/// Community ids of groups this system owns; growth.rs skips them
/// so it never recruits a band whose fate the director is rolling.
static CLAIMED: Mutex<Vec<i64>> = Mutex::new(Vec::new());
static LAST_TICK_BITS: AtomicU32 = AtomicU32::new(0);

/// Check whether another system already controls an arriving group.
/// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
pub fn is_claimed(id: i64) -> bool {
    CLAIMED.lock().contains(&id)
}

/// Count the encounters or missions currently in flight.
/// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
pub fn active_count() -> usize {
    MISSIONS.lock().len()
}

/// Advance this system when its scheduled game update is due.
/// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
pub fn tick(now: f32) {
    if mission::should_tick(now, MISSION_TICK_SECS, &LAST_TICK_BITS) {
        mission::advance_one_stage_all(&MISSIONS, now, |mission, error| {
            mono::log(
                LogLevel::Warn,
                &format!(
                    "survivalist-mod: stranger -- resolve failed for the band near {}: {error}",
                    mission.target_name
                ),
            );
        });
    }
}

// ---- launching (the storyteller rule) --------------------------------------

/// Match an unclaimed arriving group to a camp and assign its hidden intent.
/// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
fn launch_with(now: f32, forced: Option<Intent>) -> Result<Outcome, String> {
    {
        let ms = MISSIONS.lock();
        let refugees = ms
            .iter()
            .filter(|m| matches!(m.intent, Intent::Refugee))
            .count();
        let bands = ms.len() - refugees;
        if matches!(forced, Some(Intent::Refugee)) {
            if refugees >= WAVE_MAX {
                return Ok(Outcome::Passed);
            }
        } else if bands >= MAX_STRANGERS {
            return Ok(Outcome::Passed);
        }
    }

    // Targets: every living settlement (and the player), kept with
    // base-centre tile coords to pick the nearest to where the band
    // crosses and to march it there.
    let mut camps: Vec<(i32, (i64, i64), String)> = Vec::new();
    for_each_community(|com| {
        let t = ctype(&com);
        let is_player = t == "Player";
        if !is_player && t != "Normal" && t != "Looter" {
            return Ok(true);
        }
        if !is_player && com.invoke("IsAISettlement", &json!([]))? != json!(true) {
            return Ok(true);
        }
        if com
            .invoke("GetLivingNonZombieMemberCount", &json!([]))?
            .as_i64()
            .unwrap_or(0)
            == 0
        {
            return Ok(true);
        }
        let Some(c) = base_centre(&com) else {
            return Ok(true);
        };
        camps.push((com.handle().0, c, display_name(&com)));
        std::mem::forget(com);
        Ok(true)
    })?;
    if camps.is_empty() {
        return Ok(Outcome::Passed);
    }

    // Spawn a REAL band at the edge from the undefined beyond (the
    // off-map generator + fresh-people/loot faucet). The mysterious
    // stranger is a lone figure.
    let lone = matches!(forced, Some(Intent::Mysterious));
    let (min, max) = if lone { (1, 1) } else { (3, 6) };
    let salt = 30 + MISSIONS.lock().len() as u64;
    let Some((band_h, band_id, spawn_tile)) =
        crate::incursion::spawn_band_at_edge(now, salt, "RovingRefugee", min, max, false)?
    else {
        for (ch, _, _) in &camps {
            drop(own(*ch));
        }
        return Ok(Outcome::Passed);
    };

    // Nearest camp to where the band crossed; march the band to it.
    let mut best = 0usize;
    let mut best_d = i64::MAX;
    for (i, (_, c, _)) in camps.iter().enumerate() {
        let d = (c.0 - spawn_tile.0).pow(2) + (c.1 - spawn_tile.1).pow(2);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    let (target_h, target_tile, target_name) = {
        let (h, t, n) = &camps[best];
        (*h, *t, n.clone())
    };
    let _ = march_band_to(band_h, target_tile, "Travel");

    CLAIMED.lock().push(band_id);
    let intent = forced.unwrap_or_else(|| roll_intent(band_id, now));
    MISSIONS.lock().push(Mission {
        group_h: band_h,
        group_id: band_id,
        target_h,
        target_name: target_name.clone(),
        intent,
        deadline: now + MISSION_TIMEOUT_SECS,
    });

    let (announce, log_shape) = match intent {
        Intent::Mysterious => (
            announce_lone_line(band_id, &target_name),
            "a lone figure crossed the edge toward {} (meaning hidden)",
        ),
        Intent::Refugee => (
            announce_refugee_line(band_id, &target_name),
            "refugees crossed the edge toward {} (fleeing something off-map)",
        ),
        _ => (
            announce_line(band_id, &target_name),
            "an unknown band crossed the edge toward {} (intent hidden)",
        ),
    };
    crate::chronicle::post(&announce);
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: stranger -- {}",
            log_shape.replace("{}", &target_name)
        ),
    );

    // Release the camp handles we did not keep as the target.
    for (i, (ch, _, _)) in camps.iter().enumerate() {
        if i != best {
            drop(own(*ch));
        }
    }
    Ok(Outcome::Fired)
}

/// Roll whether strangers join, share, threaten, rob, or leave.
/// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
fn roll_intent(id: i64, now: f32) -> Intent {
    // Out of 100: join 30, share 12, aggressive 30, leave 13,
    // shakedown 15.
    match hash_pick(id, now, 100) {
        0..=29 => Intent::FriendlyJoin,
        30..=41 => Intent::FriendlyShare,
        42..=71 => Intent::Aggressive,
        72..=84 => Intent::WaryLeave,
        _ => Intent::Shakedown,
    }
}

/// A pseudo-random value in [0, n): a hash of a stable id and a
/// salt, so a band's roll is unpredictable but its flavor lines
/// stay consistent.
/// Extraction candidate: Modforge should own deterministic salted selection; Survivalist should supply encounter identities, weights, and text.
fn hash_pick(id: i64, salt: f32, n: u64) -> u64 {
    let mut h = (id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (salt.to_bits() as u64).wrapping_mul(0xD1B5_4A32_D192_ED03);
    h ^= h >> 29;
    h % n.max(1)
}

/// Read a camp position for matching arrivals to destinations.
/// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
fn camp_pos(com: &MonoObject) -> Option<(f32, f32)> {
    let b_h = com
        .read_field("Buildings")
        .ok()
        .as_ref()
        .and_then(handle_of)?;
    let blist = own(b_h);
    if blist.list_len_or_zero().ok()? == 0 {
        return None;
    }
    let anchor_h = blist.list_handle(0).ok().flatten()?;
    pos_of(&own(anchor_h))
}

// ---- resolving -------------------------------------------------------------

impl mission::OneStageMission for Mission {
    /// Advance the active contract or mission and resolve its next outcome.
    /// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
    fn advance(&mut self, now: f32) -> Result<OneStageStep, String> {
        let members = with(self.group_h, |g| {
            g.invoke("GetLivingNonZombieMemberCount", &json!([]))
                .ok()
                .and_then(|v| v.as_i64())
        })
        .unwrap_or(0);
        if members <= 0 {
            return Ok(OneStageStep::Complete);
        }
        if now >= self.deadline {
            return Ok(OneStageStep::TimedOut);
        }

        let lead_h = with(self.group_h, |g| -> Option<i32> {
            handle_of(&g.read_field("Leader").ok()?)
        });
        let Some(lead_h) = lead_h else {
            return Ok(OneStageStep::Continue);
        };
        let Some(gpos) = pos_of(&own(lead_h)) else {
            return Ok(OneStageStep::Continue);
        };
        let Some(cpos) = with(self.target_h, camp_pos) else {
            return Ok(OneStageStep::Complete); // the camp is gone
        };
        let (dx, dy) = (gpos.0 - cpos.0, gpos.1 - cpos.1);
        if dx * dx + dy * dy > RESOLVE_RANGE * RESOLVE_RANGE {
            return Ok(OneStageStep::Continue); // not at the gate yet
        }

        match self.intent {
            Intent::FriendlyJoin => {
                let joined = join_target(self)?;
                crate::chronicle::post(&reveal_friendly(&self.target_name, joined));
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: stranger -- FRIENDLY: {joined} joined {}",
                        self.target_name
                    ),
                );
            }
            Intent::FriendlyShare => {
                let shared = gift_from_band(self, SHARE_STACKS)?;
                crate::chronicle::post(&reveal_share(self.group_id, &self.target_name, shared));
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: stranger -- FRIENDLY (trader): shared {shared} good(s) with {}",
                        self.target_name
                    ),
                );
            }
            Intent::Aggressive => {
                set_hostile(self)?;
                crate::chronicle::post(&reveal_aggressive(self.group_id, &self.target_name));
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: stranger -- AGGRESSIVE: a band falls on {}",
                        self.target_name
                    ),
                );
            }
            Intent::Mysterious => {
                // The outcome is real, but no line ever explains it:
                // the lingering mystery IS the payoff.
                match hash_pick(self.group_id, 9.0, 3) {
                    0 => {
                        let left = gift_from_band(self, 1)?;
                        crate::chronicle::post(&reveal_mystery_gift(&self.target_name, left));
                        mono::log(
                            LogLevel::Info,
                            &format!(
                                "survivalist-mod: stranger -- MYSTERIOUS: the lone figure left {left} good(s) at {}",
                                self.target_name
                            ),
                        );
                    }
                    1 => {
                        crate::chronicle::post(&format!(
                            "a lone figure watched {} from a distance for hours; by nightfall there was no trace",
                            self.target_name
                        ));
                        mono::log(
                            LogLevel::Info,
                            &format!(
                                "survivalist-mod: stranger -- MYSTERIOUS: the lone figure watched {} and vanished",
                                self.target_name
                            ),
                        );
                    }
                    _ => {
                        crate::chronicle::post(&format!(
                            "the stranger spoke a single sentence at {}'s gate and left; no two retellings agree",
                            self.target_name
                        ));
                        mono::log(
                            LogLevel::Info,
                            &format!(
                                "survivalist-mod: stranger -- MYSTERIOUS: the lone figure spoke at {} and left",
                                self.target_name
                            ),
                        );
                    }
                }
            }
            Intent::Refugee => {
                // Shelter through the same real join path recruitment
                // uses; what they fled is never named here (the loop
                // that sent them delivers it next).
                let joined = join_target(self)?;
                crate::chronicle::post(&reveal_refugees(&self.target_name, joined));
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: stranger -- REFUGEES: {joined} sheltered at {}",
                        self.target_name
                    ),
                );
            }
            Intent::WaryLeave => {
                crate::chronicle::post(&reveal_wary(self.group_id, &self.target_name));
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: stranger -- WARY: a band sized up {} and left",
                        self.target_name
                    ),
                );
            }
            Intent::Shakedown => {
                let took = take_tribute(self)?;
                crate::chronicle::post(&reveal_shakedown(self.group_id, &self.target_name, took));
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: stranger -- SHAKEDOWN: took {took} stack(s) as tribute from {}",
                        self.target_name
                    ),
                );
            }
        }
        Ok(OneStageStep::Complete)
    }

    /// Resolve a mission that ran out of time.
    /// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
    fn on_timeout(&mut self, _now: f32) -> Result<(), String> {
        match self.intent {
            Intent::Mysterious => crate::chronicle::post(&format!(
                "the lone figure never reached {}; perhaps it was never coming",
                self.target_name
            )),
            Intent::Refugee => crate::chronicle::post(&format!(
                "the refugees scattered before reaching {}",
                self.target_name
            )),
            _ => crate::chronicle::post(&format!(
                "the strangers never reached {} and moved on",
                self.target_name
            )),
        }
        Ok(())
    }

    /// Release the mission squad and managed handles when the mission ends.
    /// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
    fn cleanup(self) {
        CLAIMED.lock().retain(|id| *id != self.group_id);
        drop(own(self.group_h));
        drop(own(self.target_h));
    }

    /// Describe the active work for status output.
    /// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
    fn label(&self) -> String {
        format!("strangers bound for {}", self.target_name)
    }
}

/// Move the band's living members into the target via the game's
/// own join path, up to the camp's real bed headroom. Returns how
/// many joined.
/// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
fn join_target(m: &Mission) -> Result<i64, String> {
    let headroom = with(m.target_h, |t| -> Result<i64, String> {
        let beds = t
            .invoke("GetAccommodation", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        let members = t
            .invoke("GetLivingNonZombieMemberCount", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        Ok((beds - members).max(0))
    })?;
    if headroom <= 0 {
        return Ok(0);
    }
    let joiners: Vec<i32> = with(m.group_h, |g| -> Result<Vec<i32>, String> {
        let mut out = Vec::new();
        let Some(m_h) = handle_of(&g.read_field("Members")?) else {
            return Ok(out);
        };
        let mlist = own(m_h);
        let count = mlist.list_len_or_zero()?;
        for i in 0..count {
            if (out.len() as i64) >= headroom {
                break;
            }
            let Some(h) = mlist.list_handle(i)? else {
                continue;
            };
            let member = own(h);
            let alive = member
                .invoke("get_AliveAndNotZombie", &json!([]))
                .map(|v| v == json!(true))
                .unwrap_or(false);
            if alive {
                out.push(member.handle().0);
                std::mem::forget(member);
            }
        }
        Ok(out)
    })?;
    let mut moved = 0i64;
    for h in joiners {
        let member = own(h);
        member.invoke("SetCommunity", &json!([{ "handle": m.target_h }]))?;
        let _ = with(m.target_h, |t| {
            t.invoke("UpdateRoles", &json!([{ "handle": member.handle().0 }]))
        });
        moved += 1;
    }
    Ok(moved)
}

/// A friendly trader band: move up to `max` non-food goods from the
/// band's members' own inventories into the camp's first building.
/// Returns how many stacks were shared (0 if the band had nothing).
/// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
fn gift_from_band(m: &Mission, max: i64) -> Result<i64, String> {
    let store: Option<(i32, i32)> = with(m.target_h, |camp| {
        let b_h = handle_of(&camp.read_field("Buildings").ok()?)?;
        let blist = own(b_h);
        let nb = blist.list_len().ok()?;
        for bi in 0..nb {
            let Some(bh) = blist.list_handle(bi).ok()? else {
                continue;
            };
            let building = own(bh);
            if let Some(inv_h) = handle_of(&building.read_field("Inventory").ok()?) {
                std::mem::forget(building);
                return Some((bh, inv_h));
            }
        }
        None
    });
    let Some((store_bh, store_inv_h)) = store else {
        return Ok(0);
    };
    let store_inv = own(store_inv_h);

    let member_hs: Vec<i32> = with(m.group_h, |g| {
        let mut out = Vec::new();
        if let Some(mlist_h) = g.read_field("Members").ok().as_ref().and_then(handle_of) {
            let mlist = own(mlist_h);
            let n = mlist.list_len().unwrap_or(0);
            for i in 0..n {
                if let Some(h) = mlist.list_handle(i).ok().flatten() {
                    out.push(h);
                }
            }
        }
        out
    });

    let mut moved = 0i64;
    for mh in member_hs {
        let member = own(mh);
        if moved < max {
            if let Some(inv_h) = member
                .read_field("Inventory")
                .ok()
                .as_ref()
                .and_then(handle_of)
            {
                let inv = own(inv_h);
                while moved < max {
                    let count = inv.list_len().unwrap_or(0);
                    let mut pick: Option<(i32, i64)> = None;
                    for j in 0..count {
                        let Some(item_h) = inv
                            .invoke("GetItem", &json!([j]))
                            .ok()
                            .as_ref()
                            .and_then(handle_of)
                        else {
                            continue;
                        };
                        let item = own(item_h);
                        let nonfood = item
                            .invoke("GetNutrition", &json!([]))
                            .ok()
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0)
                            <= 0.0;
                        let amount = item
                            .invoke("GetAmount", &json!([]))
                            .ok()
                            .and_then(|v| v.as_i64())
                            .unwrap_or(1);
                        if nonfood {
                            std::mem::forget(item);
                            pick = Some((item_h, amount));
                            break;
                        }
                    }
                    let Some((item_h, amount)) = pick else { break };
                    let taken = inv.invoke(
                        "Take",
                        &json!([{ "handle": mh }, { "handle": item_h }, amount]),
                    )?;
                    let Some(taken_h) = handle_of(&taken) else {
                        break;
                    };
                    store_inv.invoke(
                        "Add",
                        &json!([{ "handle": store_bh }, { "handle": taken_h }]),
                    )?;
                    moved += 1;
                }
            }
        }
        drop(member);
    }
    drop(store_inv);
    drop(own(store_bh));
    Ok(moved)
}

/// A shakedown: the band takes up to `TRIBUTE_STACKS` non-food
/// stacks from the CAMP's own stores into a band member, then
/// leaves. Returns how many stacks were taken.
/// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
fn take_tribute(m: &Mission) -> Result<i64, String> {
    let carrier = with(m.group_h, |g| -> Option<i32> {
        if let Some(h) = handle_of(&g.read_field("Leader").ok()?) {
            return Some(h);
        }
        let mlist_h = handle_of(&g.read_field("Members").ok()?)?;
        let mlist = own(mlist_h);
        let n = mlist.list_len().ok()?;
        for i in 0..n {
            if let Some(h) = mlist.list_handle(i).ok().flatten() {
                return Some(h);
            }
        }
        None
    });
    let Some(carrier) = carrier else {
        return Ok(0);
    };
    let took = with(m.target_h, |camp| {
        carry_off_stored_goods(camp, &[carrier], TRIBUTE_STACKS, GoodsFilter::NonFood, true)
    })?;
    drop(own(carrier));
    Ok(took)
}

/// Set the band hostile to the target and, best-effort, make them
/// actively invade (the same calls war_ignite uses). The game's own
/// combat AI carries it from here.
/// Stays here because it applies Survivalist's stranger encounters rules through the game's classes, fields, content, and actions.
fn set_hostile(m: &Mission) -> Result<(), String> {
    let cm = community_manager()?;
    cm.invoke(
        "SetRelationship",
        &json!([{ "handle": m.group_h }, { "handle": m.target_h }, "Hostile"]),
    )?;
    let _ = with(m.group_h, |g| {
        g.invoke(
            "SetInvasionTarget",
            &json!([{ "handle": m.target_h }, 7.0, false]),
        )
    });
    Ok(())
}

// ---- flavor (variety so the reveal never goes stale) -----------------------

/// Choose the first rumor announcing an unknown group near a camp.
/// Stays here because the encounter wording is Survivalist content.
fn announce_line(id: i64, camp: &str) -> String {
    const L: &[&str] = &[
        "strangers are approaching {}",
        "an unknown band moves toward {}",
        "figures on the road near {}, their intent unclear",
    ];
    L[hash_pick(id, 0.0, L.len() as u64) as usize].replace("{}", camp)
}

/// Choose the first rumor announcing a lone mysterious figure.
/// Stays here because the encounter wording is Survivalist content.
fn announce_lone_line(id: i64, camp: &str) -> String {
    const L: &[&str] = &[
        "a lone figure is walking toward {}",
        "someone approaches {} alone, and no one knows them",
        "a single traveller nears {}, saying nothing",
    ];
    L[hash_pick(id, 0.0, L.len() as u64) as usize].replace("{}", camp)
}

/// Choose the first rumor announcing refugees near a camp.
/// Stays here because the encounter wording is Survivalist content.
fn announce_refugee_line(id: i64, camp: &str) -> String {
    const L: &[&str] = &[
        "refugees are coming down the road toward {}, looking behind them",
        "a ragged band nears {}, running from something",
        "families on the road make for {}, carrying what they could",
    ];
    L[hash_pick(id, 0.0, L.len() as u64) as usize].replace("{}", camp)
}

/// Describe how many refugees a camp accepted.
/// Stays here because the encounter wording is Survivalist content.
fn reveal_refugees(camp: &str, n: i64) -> String {
    if n > 0 {
        format!("refugees found shelter at {camp}; they will not speak of what they fled")
    } else {
        format!("refugees begged at {camp}'s gate, but there was no room, and they moved on")
    }
}

/// Describe the unexplained gift left by a mysterious stranger.
/// Stays here because the encounter wording is Survivalist content.
fn reveal_mystery_gift(camp: &str, n: i64) -> String {
    if n > 0 {
        format!("the stranger left something at {camp}'s gate and walked away without a word")
    } else {
        format!("the stranger stood a while at {camp}'s gate, said nothing, and moved on")
    }
}

/// Describe strangers who arrived ready to join.
/// Stays here because the encounter wording is Survivalist content.
fn reveal_friendly(camp: &str, n: i64) -> String {
    if n > 0 {
        format!("the strangers came in peace: {n} threw in with {camp}")
    } else {
        format!("the strangers sought shelter at {camp}, but there was no room")
    }
}

/// Describe the goods a friendly band shared.
/// Stays here because the encounter wording is Survivalist content.
fn reveal_share(id: i64, camp: &str, n: i64) -> String {
    if n > 0 {
        const L: &[&str] = &[
            "the strangers proved traders: they left {} some supplies and moved on",
            "the band shared what they could spare with {} and kept walking",
        ];
        L[hash_pick(id, 7.0, L.len() as u64) as usize].replace("{}", camp)
    } else {
        format!("the strangers were friendly but had nothing to spare for {camp}")
    }
}

/// Describe strangers revealing hostile intent.
/// Stays here because the encounter wording is Survivalist content.
fn reveal_aggressive(id: i64, camp: &str) -> String {
    const L: &[&str] = &[
        "the strangers came for blood: {} is under attack",
        "it was a raid; the band fell on {}",
        "the newcomers drew blades at {}'s gate",
    ];
    L[hash_pick(id, 7.0, L.len() as u64) as usize].replace("{}", camp)
}

/// Describe cautious strangers deciding to leave.
/// Stays here because the encounter wording is Survivalist content.
fn reveal_wary(id: i64, camp: &str) -> String {
    const L: &[&str] = &[
        "the strangers sized up {} and moved on",
        "the band eyed {}'s walls and kept walking",
        "the newcomers thought better of {} and left",
    ];
    L[hash_pick(id, 7.0, L.len() as u64) as usize].replace("{}", camp)
}

/// Describe the tribute taken by threatening strangers.
/// Stays here because the encounter wording is Survivalist content.
fn reveal_shakedown(id: i64, camp: &str, n: i64) -> String {
    if n > 0 {
        const L: &[&str] = &[
            "the strangers demanded tribute: {} paid a stack to see them gone",
            "the band shook {} down for goods and left",
        ];
        L[hash_pick(id, 7.0, L.len() as u64) as usize].replace("{}", camp)
    } else {
        format!("the strangers menaced {camp} for tribute but found nothing worth taking")
    }
}
