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

use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::json;

use unityforge::mono::{self, LogLevel, MonoObject};

use crate::common::{
    GoodsFilter, carry_off_stored_goods, community_manager, ctype, display_name,
    for_each_community, handle_of, own, pos_of, with,
};
use crate::storyteller::Outcome;

/// Force-launch a stranger now with a ROLLED hidden intent,
/// reporting whether a group was near enough to actually cross. The
/// incursion loop drives this, so every stranger arrives foreshadowed
/// by off-map dread.
pub fn launch_now(now: f32) -> bool {
    matches!(launch_with(now, None), Ok(Outcome::Fired))
}

/// Force-launch a band whose intent is fixed HOSTILE: off-map
/// raiders that mean to attack. Same arrival machinery, no roll.
pub fn launch_raiders(now: f32) -> bool {
    matches!(launch_with(now, Some(Intent::Aggressive)), Ok(Outcome::Fired))
}

/// Seconds between resolve passes (arrival checks).
const MISSION_TICK_SECS: f32 = 5.0;

/// A group already this close (world units) to a camp can be
/// claimed as an approaching band.
const ARRIVING_RANGE: f32 = 140.0;

/// The band has reached the gate within this range: reveal the
/// intent.
const RESOLVE_RANGE: f32 = 48.0;

/// A band that has not reached its camp by then wandered off.
const MISSION_TIMEOUT_SECS: f32 = 900.0;

/// At most this many bands in play map-wide.
const MAX_STRANGERS: usize = 2;

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

pub fn is_claimed(id: i64) -> bool {
    CLAIMED.lock().contains(&id)
}

pub fn active_count() -> usize {
    MISSIONS.lock().len()
}

pub fn tick(now: f32) {
    let last = f32::from_bits(LAST_TICK_BITS.load(Ordering::Relaxed));
    if now - last >= MISSION_TICK_SECS {
        LAST_TICK_BITS.store(now.to_bits(), Ordering::Relaxed);
        advance(now);
    }
}

// ---- launching (the storyteller rule) --------------------------------------

struct Camp {
    handle: i32,
    name: String,
    pos: (f32, f32),
}

struct Group {
    handle: i32,
    id: i64,
    pos: (f32, f32),
}

fn launch_with(now: f32, forced: Option<Intent>) -> Result<Outcome, String> {
    if MISSIONS.lock().len() >= MAX_STRANGERS {
        return Ok(Outcome::Passed);
    }

    let mut camps: Vec<Camp> = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    for_each_community(|com| {
        let t = ctype(&com);
        if t == "RovingRefugee" {
            let members = com
                .invoke("GetLivingNonZombieMemberCount", &json!([]))?
                .as_i64()
                .unwrap_or(0);
            let id = com.read_field("Id")?.as_i64().unwrap_or(-1);
            if members > 0 && !is_claimed(id) {
                if let Some(lead_h) = handle_of(&com.read_field("Leader")?) {
                    if let Some(pos) = pos_of(&own(lead_h)) {
                        groups.push(Group {
                            handle: com.handle().0,
                            id,
                            pos,
                        });
                        std::mem::forget(com);
                        return Ok(true);
                    }
                }
            }
            return Ok(true);
        }
        let is_player = t == "Player";
        if !is_player && t != "Normal" && t != "Looter" {
            return Ok(true);
        }
        if !is_player && com.invoke("IsAISettlement", &json!([]))? != json!(true) {
            return Ok(true);
        }
        let members = com
            .invoke("GetLivingNonZombieMemberCount", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        if members == 0 {
            return Ok(true);
        }
        let Some(pos) = camp_pos(&com) else {
            return Ok(true);
        };
        camps.push(Camp {
            handle: com.handle().0,
            name: display_name(&com),
            pos,
        });
        std::mem::forget(com);
        Ok(true)
    })?;

    let result = try_launch(&groups, &camps, now, forced);

    let kept: Vec<i32> = {
        let ms = MISSIONS.lock();
        ms.iter().flat_map(|m| [m.group_h, m.target_h]).collect()
    };
    for g in &groups {
        if !kept.contains(&g.handle) {
            drop(own(g.handle));
        }
    }
    for c in &camps {
        if !kept.contains(&c.handle) {
            drop(own(c.handle));
        }
    }
    result
}

fn try_launch(
    groups: &[Group],
    camps: &[Camp],
    now: f32,
    forced: Option<Intent>,
) -> Result<Outcome, String> {
    if groups.is_empty() || camps.is_empty() {
        return Ok(Outcome::Passed);
    }
    let mut best: Option<(&Group, &Camp, f32)> = None;
    for g in groups {
        for c in camps {
            let (dx, dy) = (g.pos.0 - c.pos.0, g.pos.1 - c.pos.1);
            let d2 = dx * dx + dy * dy;
            if d2 <= ARRIVING_RANGE * ARRIVING_RANGE
                && best.map(|(_, _, bd)| d2 < bd).unwrap_or(true)
            {
                best = Some((g, c, d2));
            }
        }
    }
    let Some((group, target, _)) = best else {
        return Ok(Outcome::Passed);
    };

    let intent = forced.unwrap_or_else(|| roll_intent(group.id, now));
    CLAIMED.lock().push(group.id);
    MISSIONS.lock().push(Mission {
        group_h: group.handle,
        group_id: group.id,
        target_h: target.handle,
        target_name: target.name.clone(),
        intent,
        deadline: now + MISSION_TIMEOUT_SECS,
    });
    crate::chronicle::post(&announce_line(group.id, &target.name));
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: stranger -- an unknown band nears {} (intent hidden)",
            target.name
        ),
    );
    Ok(Outcome::Fired)
}

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
fn hash_pick(id: i64, salt: f32, n: u64) -> u64 {
    let mut h = (id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (salt.to_bits() as u64).wrapping_mul(0xD1B5_4A32_D192_ED03);
    h ^= h >> 29;
    h % n.max(1)
}

fn camp_pos(com: &MonoObject) -> Option<(f32, f32)> {
    let b_h = com.read_field("Buildings").ok().as_ref().and_then(handle_of)?;
    let blist = own(b_h);
    if blist.invoke("get_Count", &json!([])).ok()?.as_i64().unwrap_or(0) == 0 {
        return None;
    }
    let anchor_h = handle_of(&blist.invoke("get_Item", &json!([0])).ok()?)?;
    pos_of(&own(anchor_h))
}

// ---- resolving -------------------------------------------------------------

fn advance(now: f32) {
    let mut missions = MISSIONS.lock();
    let mut i = 0;
    while i < missions.len() {
        let done = match resolve(&missions[i], now) {
            Ok(d) => d,
            Err(e) => {
                mono::log(
                    LogLevel::Warn,
                    &format!(
                        "survivalist-mod: stranger -- resolve failed for the band near {}: {e}",
                        missions[i].target_name
                    ),
                );
                true
            }
        };
        if done {
            let m = missions.remove(i);
            CLAIMED.lock().retain(|id| *id != m.group_id);
            drop(own(m.group_h));
            drop(own(m.target_h));
        } else {
            i += 1;
        }
    }
}

fn resolve(m: &Mission, now: f32) -> Result<bool, String> {
    let members = with(m.group_h, |g| {
        g.invoke("GetLivingNonZombieMemberCount", &json!([]))
            .ok()
            .and_then(|v| v.as_i64())
    })
    .unwrap_or(0);
    if members <= 0 {
        return Ok(true);
    }
    if now >= m.deadline {
        crate::chronicle::post(&format!(
            "the strangers never reached {} and moved on",
            m.target_name
        ));
        return Ok(true);
    }

    let lead_h = with(m.group_h, |g| -> Option<i32> {
        handle_of(&g.read_field("Leader").ok()?)
    });
    let Some(lead_h) = lead_h else {
        return Ok(false);
    };
    let Some(gpos) = pos_of(&own(lead_h)) else {
        return Ok(false);
    };
    let Some(cpos) = with(m.target_h, camp_pos) else {
        return Ok(true); // the camp is gone
    };
    let (dx, dy) = (gpos.0 - cpos.0, gpos.1 - cpos.1);
    if dx * dx + dy * dy > RESOLVE_RANGE * RESOLVE_RANGE {
        return Ok(false); // not at the gate yet
    }

    match m.intent {
        Intent::FriendlyJoin => {
            let joined = join_target(m)?;
            crate::chronicle::post(&reveal_friendly(&m.target_name, joined));
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: stranger -- FRIENDLY: {joined} joined {}",
                    m.target_name
                ),
            );
        }
        Intent::FriendlyShare => {
            let shared = gift_from_band(m, SHARE_STACKS)?;
            crate::chronicle::post(&reveal_share(m.group_id, &m.target_name, shared));
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: stranger -- FRIENDLY (trader): shared {shared} good(s) with {}",
                    m.target_name
                ),
            );
        }
        Intent::Aggressive => {
            set_hostile(m)?;
            crate::chronicle::post(&reveal_aggressive(m.group_id, &m.target_name));
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: stranger -- AGGRESSIVE: a band falls on {}",
                    m.target_name
                ),
            );
        }
        Intent::WaryLeave => {
            crate::chronicle::post(&reveal_wary(m.group_id, &m.target_name));
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: stranger -- WARY: a band sized up {} and left",
                    m.target_name
                ),
            );
        }
        Intent::Shakedown => {
            let took = take_tribute(m)?;
            crate::chronicle::post(&reveal_shakedown(m.group_id, &m.target_name, took));
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: stranger -- SHAKEDOWN: took {took} stack(s) as tribute from {}",
                    m.target_name
                ),
            );
        }
    }
    Ok(true)
}

/// Move the band's living members into the target via the game's
/// own join path, up to the camp's real bed headroom. Returns how
/// many joined.
fn join_target(m: &Mission) -> Result<i64, String> {
    let headroom = with(m.target_h, |t| -> Result<i64, String> {
        let beds = t.invoke("GetAccommodation", &json!([]))?.as_i64().unwrap_or(0);
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
        let count = mlist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
        for i in 0..count {
            if (out.len() as i64) >= headroom {
                break;
            }
            let Some(h) = handle_of(&mlist.invoke("get_Item", &json!([i]))?) else {
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
fn gift_from_band(m: &Mission, max: i64) -> Result<i64, String> {
    let store: Option<(i32, i32)> = with(m.target_h, |camp| {
        let b_h = handle_of(&camp.read_field("Buildings").ok()?)?;
        let blist = own(b_h);
        let nb = blist.invoke("get_Count", &json!([])).ok()?.as_i64()?;
        for bi in 0..nb {
            let Some(bh) = handle_of(&blist.invoke("get_Item", &json!([bi])).ok()?) else {
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
            let n = mlist
                .invoke("get_Count", &json!([]))
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            for i in 0..n {
                if let Some(h) = mlist
                    .invoke("get_Item", &json!([i]))
                    .ok()
                    .as_ref()
                    .and_then(handle_of)
                {
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
            if let Some(inv_h) = member.read_field("Inventory").ok().as_ref().and_then(handle_of) {
                let inv = own(inv_h);
                while moved < max {
                    let count = inv
                        .invoke("get_Count", &json!([]))
                        .ok()
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
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
                    let Some(taken_h) = handle_of(&taken) else { break };
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
fn take_tribute(m: &Mission) -> Result<i64, String> {
    let carrier = with(m.group_h, |g| -> Option<i32> {
        if let Some(h) = handle_of(&g.read_field("Leader").ok()?) {
            return Some(h);
        }
        let mlist_h = handle_of(&g.read_field("Members").ok()?)?;
        let mlist = own(mlist_h);
        let n = mlist.invoke("get_Count", &json!([])).ok()?.as_i64()?;
        for i in 0..n {
            if let Some(h) = mlist
                .invoke("get_Item", &json!([i]))
                .ok()
                .as_ref()
                .and_then(handle_of)
            {
                return Some(h);
            }
        }
        None
    });
    let Some(carrier) = carrier else {
        return Ok(0);
    };
    let took = with(m.target_h, |camp| {
        carry_off_stored_goods(camp, &[carrier], TRIBUTE_STACKS, GoodsFilter::NonFood)
    })?;
    drop(own(carrier));
    Ok(took)
}

/// Set the band hostile to the target and, best-effort, make them
/// actively invade (the same calls war_ignite uses). The game's own
/// combat AI carries it from here.
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

fn announce_line(id: i64, camp: &str) -> String {
    const L: &[&str] = &[
        "strangers are approaching {}",
        "an unknown band moves toward {}",
        "figures on the road near {}, their intent unclear",
    ];
    L[hash_pick(id, 0.0, L.len() as u64) as usize].replace("{}", camp)
}

fn reveal_friendly(camp: &str, n: i64) -> String {
    if n > 0 {
        format!("the strangers came in peace: {n} threw in with {camp}")
    } else {
        format!("the strangers sought shelter at {camp}, but there was no room")
    }
}

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

fn reveal_aggressive(id: i64, camp: &str) -> String {
    const L: &[&str] = &[
        "the strangers came for blood: {} is under attack",
        "it was a raid; the band fell on {}",
        "the newcomers drew blades at {}'s gate",
    ];
    L[hash_pick(id, 7.0, L.len() as u64) as usize].replace("{}", camp)
}

fn reveal_wary(id: i64, camp: &str) -> String {
    const L: &[&str] = &[
        "the strangers sized up {} and moved on",
        "the band eyed {}'s walls and kept walking",
        "the newcomers thought better of {} and left",
    ];
    L[hash_pick(id, 7.0, L.len() as u64) as usize].replace("{}", camp)
}

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
