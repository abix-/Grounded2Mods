//! Clear the threat: the second work kind of the work pillar
//! (docs/status.md "More to do (ecosystem-generated work)").
//!
//! A camp with raiders at its door pays the player to wipe them
//! out. The game's own Threat records define the trouble
//! (Community.Threats: groups of hostile characters seen near the
//! base) and the game dropping the record (FindThreatById returns
//! null) defines "over". Payment is owed only if the player's
//! people killed at least one threat member while the offer
//! stood; the camp's own guards handling it, or the raiders
//! leaving, pays nothing.
//!
//! Kill attribution rides war.rs's OnMemberDied prefix (it calls
//! threat::on_death); payment rides the shared courier
//! (courier.rs); the journal entry rides the work board
//! (board.rs).

use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::mission::{self, Contract, ContractPhase};
use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::mono::{self, LogLevel, MonoObject};

use crate::common::{
    GoodsFilter, count_stored_goods, ctype, display_name, for_each_community, handle_of, list_len,
    on_main_thread, own, with,
};
use crate::{board, courier};

/// Seconds between offer scans; offset from the bounty (300) so
/// the work kinds interleave.
const THREAT_SCAN_PERIOD_SECS: f32 = 360.0;

/// Seconds between advance passes.
const MISSION_TICK_SECS: f32 = 5.0;

/// Real seconds an offer stands before it lapses.
const OFFER_WINDOW_SECS: f32 = 2700.0;

/// The quest data shipped in story/Scripts/WorkBoard.xml.
const BOARD_QUEST_ID: &str = "WorkBoard_ClearThreat";

/// The one clear-threat job in flight map-wide.
enum ClearThreat {
    Offered {
        camp_h: i32,
        camp_name: String,
        /// The game's own Threat record id; the offer resolves
        /// when the game drops it.
        threat_id: i64,
        /// The threat members at offer time: (handle, character
        /// id). Handles owned; ids are what on_death matches.
        members: Vec<(i32, i64)>,
        /// Threat members killed by the player's people while the
        /// offer stood; at least one earns the payment.
        player_kills: i64,
        pays: i64,
        quest_h: Option<i32>,
        expires: f32,
    },
    Owed {
        camp_h: i32,
        camp_name: String,
        waiting_logged: bool,
    },
    Paying(courier::Courier),
}

impl Contract for ClearThreat {
    /// Report which stage of this work contract is active.
    /// Stays here because it applies Survivalist's threat contracts rules through the game's classes, fields, content, and actions.
    fn phase(&self) -> ContractPhase {
        match self {
            ClearThreat::Offered { .. } => ContractPhase::Offered,
            ClearThreat::Owed { .. } => ContractPhase::Owed,
            ClearThreat::Paying(_) => ContractPhase::Paying,
        }
    }

    /// Advance the active contract or mission and resolve its next outcome.
    /// Stays here because it applies Survivalist's threat contracts rules through the game's classes, fields, content, and actions.
    fn advance(self, now: f32) -> Result<Option<Self>, String> {
        Ok(match self {
            ClearThreat::Offered {
                camp_h,
                camp_name,
                threat_id,
                members,
                player_kills,
                pays,
                quest_h,
                expires,
            } => advance_offered(
                camp_h,
                camp_name,
                threat_id,
                members,
                player_kills,
                pays,
                quest_h,
                expires,
                now,
            ),
            ClearThreat::Owed { camp_h, camp_name, waiting_logged } => {
                match courier::launch(
                    camp_h,
                    &camp_name,
                    &format!("the raiders at {camp_name}'s door"),
                    now,
                ) {
                    courier::Launch::Launched(c) => Some(ClearThreat::Paying(c)),
                    courier::Launch::Waiting => {
                        if !waiting_logged {
                            mono::log(
                                LogLevel::Info,
                                &format!(
                                    "survivalist-mod: threat: {camp_name} owes for the cleared threat but has no free member to send; waiting"
                                ),
                            );
                        }
                        Some(ClearThreat::Owed { camp_h, camp_name, waiting_logged: true })
                    }
                    courier::Launch::Void => {
                        drop(own(camp_h));
                        None
                    }
                }
            }
            ClearThreat::Paying(c) => courier::step(c, now).map(ClearThreat::Paying),
        })
    }

    /// Describe the active work for status output.
    /// Stays here because it applies Survivalist's threat contracts rules through the game's classes, fields, content, and actions.
    fn label(&self) -> String {
        match self {
            ClearThreat::Offered { camp_name, .. } => {
                format!("clear threat at {camp_name}")
            }
            ClearThreat::Owed { camp_name, .. } => {
                format!("threat debt from {camp_name}")
            }
            ClearThreat::Paying(c) => {
                format!("threat payment via {}", c.courier_name)
            }
        }
    }
}

static STATE: Mutex<Option<ClearThreat>> = Mutex::new(None);
static LAST_SCAN_BITS: AtomicU32 = AtomicU32::new(0);
static LAST_TICK_BITS: AtomicU32 = AtomicU32::new(0);
static LAST_NOW_BITS: AtomicU32 = AtomicU32::new(0);

/// Advance this system when its scheduled game update is due.
/// Stays here because it applies Survivalist's threat contracts rules through the game's classes, fields, content, and actions.
pub fn tick(now: f32) {
    LAST_NOW_BITS.store(now.to_bits(), Ordering::Relaxed);
    if mission::should_tick(now, MISSION_TICK_SECS, &LAST_TICK_BITS) {
        mission::advance_contract(&STATE, now, |e| {
            mono::log(LogLevel::Warn, &format!("survivalist-mod: threat advance failed: {e}"));
        });
    }
    if mission::should_tick(now, THREAT_SCAN_PERIOD_SECS, &LAST_SCAN_BITS) {
        if STATE.lock().is_some() {
            return;
        }
        if let Err(e) = offer_scan(now) {
            if !e.contains("not found") {
                mono::log(LogLevel::Warn, &format!("survivalist-mod: threat scan failed: {e}"));
            }
        }
    }
}

// ---- the offer ---------------------------------------------------------------

/// Find one live threat that can become a player job.
/// Stays here because it applies Survivalist's threat contracts rules through the game's classes, fields, content, and actions.
fn offer_scan(now: f32) -> Result<(), String> {
    board::sweep_orphans();

    // The player's camp: the work exists to be done by them.
    let mut player_h: Option<i32> = None;
    for_each_community(|com| {
        if ctype(&com) == "Player" {
            player_h = Some(com.handle().0);
            std::mem::forget(com);
            return Ok(false);
        }
        Ok(true)
    })?;
    let Some(player_h) = player_h else { return Ok(()) };

    // The hirer: an AI settlement with a live threat at its door,
    // friendly enough to the player, able to pay; the smallest
    // camp first (the least able to handle it alone).
    let mut pick: Option<(i32, String, i64)> = None; // camp_h, name, members
    for_each_community(|com| {
        let t = ctype(&com);
        if t != "Normal" && t != "Looter" {
            return Ok(true);
        }
        if com.invoke("IsAISettlement", &json!([]))? != json!(true) {
            return Ok(true);
        }
        let members = com
            .invoke("GetLivingNonZombieMemberCount", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        if members < 2 {
            return Ok(true);
        }
        if list_len(&com, "Threats") == 0 {
            return Ok(true);
        }
        let friendly = com
            .invoke("GetRelationship", &json!([{ "handle": player_h }]))
            .map(|r| r != json!("Hostile"))
            .unwrap_or(false);
        if !friendly || count_stored_goods(&com, GoodsFilter::NonFood, 1) == 0 {
            return Ok(true);
        }
        if pick.as_ref().map(|p| members < p.2).unwrap_or(true) {
            if let Some((old_h, ..)) = pick.replace((com.handle().0, display_name(&com), members)) {
                drop(own(old_h));
            }
            std::mem::forget(com);
        }
        Ok(true)
    })?;
    drop(own(player_h));
    let Some((camp_h, camp_name, _)) = pick else {
        return Ok(());
    };
    if !post_offer(camp_h, camp_name, now)? {
        // No valid threat snapshot; the camp handle was released.
    }
    Ok(())
}

/// Snapshot the camp's first valid threat and post the offer.
/// Consumes camp_h (kept in the state on success, released on
/// failure). Ok(false) = nothing posted.
/// Stays here because it applies Survivalist's threat contracts rules through the game's classes, fields, content, and actions.
fn post_offer(camp_h: i32, camp_name: String, now: f32) -> Result<bool, String> {
    // The first threat with a living, non-player membership. A
    // threat whose members include the PLAYER's people is the
    // player attacking this camp; nobody hires the mark.
    let snapshot: Option<(i64, Vec<(i32, i64)>)> = with(camp_h, |com| {
        let t_h = handle_of(&com.read_field("Threats").ok()?)?;
        let tlist = own(t_h);
        let nt = tlist.invoke("get_Count", &json!([])).ok()?.as_i64()?;
        'threats: for ti in 0..nt {
            let Some(th) = handle_of(&tlist.invoke("get_Item", &json!([ti])).ok()?) else {
                continue;
            };
            let threat = own(th);
            let threat_id = threat.read_field("Id").ok()?.as_i64()?;
            let Some(m_h) = handle_of(&threat.read_field("ThreatMembers").ok()?) else {
                continue;
            };
            let mlist = own(m_h);
            let nm = mlist.invoke("get_Count", &json!([])).ok()?.as_i64()?;
            let mut members: Vec<(i32, i64)> = Vec::new();
            for mi in 0..nm {
                let Some(mh) = handle_of(&mlist.invoke("get_Item", &json!([mi])).ok()?) else {
                    continue;
                };
                let member = own(mh);
                let alive = member
                    .invoke("get_AliveAndNotZombie", &json!([]))
                    .map(|v| v == json!(true))
                    .unwrap_or(false);
                if !alive {
                    continue;
                }
                // Player-community member in the threat: skip the
                // whole threat.
                let is_player = member
                    .read_field("Community")
                    .ok()
                    .as_ref()
                    .and_then(handle_of)
                    .map(|ch| ctype(&own(ch)) == "Player")
                    .unwrap_or(false);
                if is_player {
                    for (h, _) in members.drain(..) {
                        drop(own(h));
                    }
                    continue 'threats;
                }
                let id = member.read_field("Id").ok().and_then(|v| v.as_i64()).unwrap_or(-1);
                std::mem::forget(member);
                members.push((mh, id));
            }
            if members.is_empty() {
                continue;
            }
            return Some((threat_id, members));
        }
        None
    });
    let Some((threat_id, members)) = snapshot else {
        drop(own(camp_h));
        return Ok(false);
    };

    let pays = with(camp_h, |com| {
        count_stored_goods(com, GoodsFilter::NonFood, courier::PAY_STACKS)
    });
    if pays == 0 {
        for (h, _) in members {
            drop(own(h));
        }
        drop(own(camp_h));
        return Ok(false);
    }

    // The board marker tracks the first raider.
    let quest_h = board::spawn(BOARD_QUEST_ID, camp_h, members[0].0);

    crate::chronicle::post(&format!(
        "{camp_name} will pay to have the raiders at their door wiped out: pays {pays} stack(s) of goods"
    ));
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: threat: {camp_name} posts clear-the-threat work ({} raiders, threat id {threat_id}), paying {pays} stack(s) (window {OFFER_WINDOW_SECS}s)",
            members.len(),
        ),
    );
    *STATE.lock() = Some(ClearThreat::Offered {
        camp_h,
        camp_name,
        threat_id,
        members,
        player_kills: 0,
        pays,
        quest_h,
        expires: now + OFFER_WINDOW_SECS,
    });
    Ok(true)
}

// ---- kill attribution ---------------------------------------------------------

/// Called from war.rs's OnMemberDied prefix for every death:
/// counts threat members felled by the player's people while the
/// offer stands. Resolution stays in the tick (the game dropping
/// the Threat record), never here.
/// Stays here because it applies Survivalist's threat contracts rules through the game's classes, fields, content, and actions.
pub fn on_death(member: &MonoObject) {
    {
        let slot = STATE.lock();
        if !matches!(slot.as_ref(), Some(ClearThreat::Offered { .. })) {
            return;
        }
    }
    let Some(dead_id) = member.read_field("Id").ok().and_then(|v| v.as_i64()) else {
        return;
    };
    let mut slot = STATE.lock();
    let Some(ClearThreat::Offered { camp_name, members, player_kills, .. }) = slot.as_mut() else {
        return;
    };
    if !members.iter().any(|(_, id)| *id == dead_id) {
        return;
    }
    let by_player = (|| {
        let kh = handle_of(&member.read_field("Killer").ok()?)?;
        let killer = own(kh);
        let ch = handle_of(&killer.read_field("Community").ok()?)?;
        Some(ctype(&own(ch)) == "Player")
    })()
    .unwrap_or(false);
    if by_player {
        *player_kills += 1;
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: threat: a raider at {camp_name}'s door fell to the player ({player_kills} so far)"
            ),
        );
    }
}

/// One offered step: lapse, camp death, or the game dropping the
/// Threat record (over: paid if the player drew blood).
/// Stays here because it applies Survivalist's threat contracts rules through the game's classes, fields, content, and actions.
#[allow(clippy::too_many_arguments)]
fn advance_offered(
    camp_h: i32,
    camp_name: String,
    threat_id: i64,
    members: Vec<(i32, i64)>,
    player_kills: i64,
    pays: i64,
    quest_h: Option<i32>,
    expires: f32,
    now: f32,
) -> Option<ClearThreat> {
    let release_members = |members: &Vec<(i32, i64)>| {
        for &(h, _) in members {
            drop(own(h));
        }
    };
    if now >= expires {
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: threat: the clear-the-threat offer at {camp_name} lapses unclaimed"
            ),
        );
        board::close(quest_h, false);
        release_members(&members);
        drop(own(camp_h));
        return None;
    }
    let standing = with(camp_h, |c| {
        c.invoke("HasAnyLivingNonZombieMembers", &json!([]))
            .map(|v| v == json!(true))
            .unwrap_or(false)
    });
    if !standing {
        mono::log(
            LogLevel::Info,
            &format!("survivalist-mod: threat: {camp_name} fell; the offer dies with them"),
        );
        board::close(quest_h, false);
        release_members(&members);
        drop(own(camp_h));
        return None;
    }
    // The game's own verdict: the Threat record gone = over.
    let threat_gone = with(camp_h, |c| {
        match c.invoke("FindThreatById", &json!([threat_id])) {
            Ok(v) => match handle_of(&v) {
                Some(h) => {
                    drop(own(h));
                    false
                }
                None => true,
            },
            Err(_) => false,
        }
    });
    if threat_gone {
        release_members(&members);
        if player_kills > 0 {
            board::close(quest_h, true);
            crate::chronicle::post(&format!(
                "the raiders at {camp_name}'s door are dead; {camp_name} owes a debt"
            ));
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: threat: the threat at {camp_name} is over with {player_kills} player kill(s); {camp_name} owes payment"
                ),
            );
            return Some(ClearThreat::Owed { camp_h, camp_name, waiting_logged: false });
        }
        board::close(quest_h, false);
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: threat: the threat at {camp_name} ended without the player's hand; no debt"
            ),
        );
        drop(own(camp_h));
        return None;
    }
    Some(ClearThreat::Offered {
        camp_h,
        camp_name,
        threat_id,
        members,
        player_kills,
        pays,
        quest_h,
        expires,
    })
}

// ---- ops ---------------------------------------------------------------------

/// Expose this system status and controls through the mod control endpoint.
/// Stays here because it applies Survivalist's threat contracts rules through the game's classes, fields, content, and actions.
pub fn register_ops() {
    OP_REGISTRY.register_many([
        OpDef::new(
            "threat_status",
            "The one open clear-the-threat job (offered/owed/paying) or null. Work-pillar observability.",
            "{}",
            threat_status,
        ),
        OpDef::new(
            "threat_post",
            "Force a clear-the-threat offer from a named camp that has a live threat. Live-verification probe.",
            "{hirer: str}",
            threat_post,
        ),
    ]);
}

/// Report open threat-clearing offers, kills, rewards, and couriers.
/// Stays here because it applies Survivalist's threat contracts rules through the game's classes, fields, content, and actions.
fn threat_status(_args: &Json) -> Result<Json, String> {
    let now = f32::from_bits(LAST_NOW_BITS.load(Ordering::Relaxed));
    let slot = STATE.lock();
    Ok(match slot.as_ref() {
        None => json!({ "threat_work": null }),
        Some(ClearThreat::Offered {
            camp_name, threat_id, members, player_kills, pays, quest_h, expires, ..
        }) => json!({
            "threat_work": {
                "stage": "offered",
                "hirer": camp_name,
                "threat_id": threat_id,
                "raiders": members.len(),
                "player_kills": player_kills,
                "pays": pays,
                "board": quest_h.is_some(),
                "expires_in_secs": (expires - now).max(0.0),
            }
        }),
        Some(ClearThreat::Owed { camp_name, .. }) => json!({
            "threat_work": { "stage": "owed", "hirer": camp_name }
        }),
        Some(ClearThreat::Paying(c)) => json!({
            "threat_work": {
                "stage": "paying",
                "hirer": c.hirer_name,
                "courier": c.courier_name,
                "stacks": c.loaded,
                "leg": match c.stage {
                    courier::Stage::Going => "going",
                    courier::Stage::Returning => "returning",
                },
            }
        }),
    })
}

/// Force a threat-clearing offer for a named camp.
/// Stays here because it applies Survivalist's threat contracts rules through the game's classes, fields, content, and actions.
fn threat_post(args: &Json) -> Result<Json, String> {
    let hirer = args
        .get("hirer")
        .and_then(Json::as_str)
        .ok_or("missing arg 'hirer' (community display name)")?
        .to_string();
    on_main_thread(move || {
        if STATE.lock().is_some() {
            return Err("a clear-the-threat job is already open (threat_status)".into());
        }
        board::sweep_orphans();
        let now = f32::from_bits(LAST_NOW_BITS.load(Ordering::Relaxed));
        let mut found: Option<(i32, String)> = None;
        for_each_community(|com| {
            if display_name(&com).eq_ignore_ascii_case(&hirer) {
                let name = display_name(&com);
                if list_len(&com, "Threats") == 0 {
                    return Err(format!("'{name}' has no live threats"));
                }
                found = Some((com.handle().0, name));
                std::mem::forget(com);
                return Ok(false);
            }
            Ok(true)
        })?;
        let Some((camp_h, camp_name)) = found else {
            return Err(format!("hirer community '{hirer}' not found"));
        };
        let posted = post_offer(camp_h, camp_name.clone(), now)?;
        if !posted {
            return Err(format!(
                "'{camp_name}' posted nothing (no living non-player threat members, or empty stores)"
            ));
        }
        match &*STATE.lock() {
            Some(ClearThreat::Offered { threat_id, members, pays, .. }) => Ok(json!({
                "posted": true, "hirer": camp_name, "threat_id": threat_id,
                "raiders": members.len(), "pays": pays,
            })),
            _ => Err("state vanished after post".into()),
        }
    })
}
