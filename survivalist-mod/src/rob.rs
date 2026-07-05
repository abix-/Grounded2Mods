//! Rob: the armed-robbery act of the multidimensional repertoire
//! (docs/faction-war.md "Multidimensional factions").
//!
//! A camp whose franchise votes menace (aggression/guile blend)
//! picks a traveler crossing the map with goods worth taking, and
//! runs the GAME'S OWN ambush mission against them:
//! `Community.Ambush(lead, victim, never-teleport, party, proto)`
//! staffs a real party (Enforcers and Guards first), walks them
//! over, DEMANDS the item in a real conversation
//! (SpeechSituation.Ambush), takes it by force from whoever holds
//! it if refused, retreats if the fight turns, and walks home:
//! the entire mission loop is vanilla (the story system uses the
//! same one); the mod only makes the choice. The teleport
//! stagecraft story ambushes use is disabled by passing an
//! effectively-infinite teleport delay, so the party WALKS.
//!
//! Learning: the voters learn aggression AND guile from the
//! outcome (the loot in camp hands raises both; a dead lead
//! lowers them hard; coming home empty stings a little).

use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use unityforge::mono::{self, LogLevel};

use crate::common::{ctype, display_name, for_each_community, handle_of, list_len, own, with};
use crate::genome::{self, Trait};

/// Seconds between robbery scans.
const ROB_SCAN_PERIOD_SECS: f32 = 180.0;

/// A voter favors robbery if their aggression/guile blend clears
/// this.
const ROB_FLOOR: f64 = 0.55;

/// Ambush staffing pulls a party; the camp must be able to spare
/// one.
const ROB_MIN_MEMBERS: i64 = 5;

/// A stack must be at least this big to be worth the trip.
const ROB_MIN_AMOUNT: i64 = 2;

/// Real seconds before the robbery's outcome is judged (the
/// vanilla mission handles travel/demand/fight/return on its own
/// clock; this just waits it out).
const OUTCOME_DELAY_SECS: f32 = 420.0;

/// Passed as the ambush teleport delay: effectively never, so the
/// party walks (the teleport is story stagecraft, a cheat here).
const NEVER_TELEPORT_SECS: f64 = 1.0e9;

struct Mission {
    robber_h: i32,
    robber_id: i64,
    robber_name: String,
    victim_name: String,
    lead_h: i32,
    lead_name: String,
    proto_h: i32,
    eval_at: f32,
    voter_ids: Vec<i64>,
}

static MISSIONS: Mutex<Vec<Mission>> = Mutex::new(Vec::new());
static LAST_SCAN_BITS: AtomicU32 = AtomicU32::new(0);

/// The active robbery a faction is running, for survival_status.
pub fn active_target(faction_id: i64) -> Option<Json> {
    MISSIONS.lock().iter().find(|m| m.robber_id == faction_id).map(|m| {
        json!({
            "victim": m.victim_name,
            "lead": m.lead_name,
        })
    })
}

pub fn tick(now: f32) {
    judge_missions(now);
    let last_scan = f32::from_bits(LAST_SCAN_BITS.load(Ordering::Relaxed));
    if now - last_scan >= ROB_SCAN_PERIOD_SECS {
        LAST_SCAN_BITS.store(now.to_bits(), Ordering::Relaxed);
        if let Err(e) = launch_scan(now) {
            if !e.contains("not found") {
                mono::log(LogLevel::Warn, &format!("survivalist-mod: rob scan failed: {e}"));
            }
        }
    }
}

// ---- launching ---------------------------------------------------------------

fn launch_scan(now: f32) -> Result<(), String> {
    // The robber: a peaceful-but-menacing camp whose franchise
    // votes it. One robbery in flight per camp, one launch per
    // scan.
    let active: Vec<i64> = MISSIONS.lock().iter().map(|m| m.robber_id).collect();
    let mut robber: Option<(i32, i64, String, String, i64, i64, f64, Vec<i64>)> = None;
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
        let id = com.read_field("Id")?.as_i64().unwrap_or(-1);
        let at_war = handle_of(&com.read_field("InvasionTarget")?).is_some();
        if members < ROB_MIN_MEMBERS || at_war || list_len(&com, "Threats") > 0 || active.contains(&id)
        {
            return Ok(true);
        }
        // One party in the field at a time, judged from GAME
        // state, not the Rust mission list: a hot reload wipes the
        // list while the ambush squad marches on (which double-
        // launched Almighty Rock Family, live 2026-07-05).
        if let Some(s_h) = handle_of(&com.read_field("Squads")?) {
            let slist = own(s_h);
            let n = slist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
            for i in 0..n {
                let Some(h) = handle_of(&slist.invoke("get_Item", &json!([i]))?) else {
                    continue;
                };
                if own(h).read_field("Behaviour").ok() == Some(json!("Ambush")) {
                    return Ok(true);
                }
            }
        }
        // The menace ballot: aggression/guile blend.
        let looter = t == "Looter";
        let mut votes = 0i64;
        let mut franchise = 0i64;
        let mut sum = 0.0f64;
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
                if looter && genome::is_conscript(char_id) {
                    continue;
                }
                let g = genome::individual(char_id, &t);
                let s = (g.get(Trait::Aggression) + g.get(Trait::Guile)) / 2.0;
                franchise += 1;
                sum += s;
                if s >= ROB_FLOOR {
                    votes += 1;
                }
                voter_ids.push(char_id);
            }
        }
        if franchise > 0 && votes * 2 > franchise {
            let eff = sum / franchise as f64;
            if robber.as_ref().map(|r| eff > r.6).unwrap_or(true) {
                if let Some(old) = robber.replace((
                    com.handle().0,
                    id,
                    display_name(&com),
                    t,
                    members,
                    votes,
                    eff,
                    voter_ids,
                )) {
                    drop(own(old.0));
                }
                std::mem::forget(com);
            }
        }
        Ok(true)
    })?;
    let Some((robber_h, robber_id, robber_name, robber_ctype, _, votes, eff, voter_ids)) = robber
    else {
        return Ok(());
    };

    // The victim: a roving traveler (trader or refugee party)
    // whose people carry a stack worth taking. Nearest is not
    // worth computing for roamers; first rich one found wins.
    let mut victim: Option<(i32, String, i32, String, i64)> = None; // (char_h, community name, proto_h, item name, amount)
    for_each_community(|com| {
        let t = ctype(&com);
        if t != "RovingTrader" && t != "RovingRefugee" {
            return Ok(true);
        }
        // Never rob a party allied to us; the ambush itself
        // resets Hostile pairs to Known.
        let victim_com_h = com.handle().0;
        let rel = with(robber_h, |r| {
            r.invoke("GetRelationship", &json!([{ "handle": victim_com_h }]))
        })
        .unwrap_or(json!("?"));
        if rel == json!("Allied") {
            return Ok(true);
        }
        let Some(m_h) = handle_of(&com.read_field("Members")?) else {
            return Ok(true);
        };
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
            if !alive {
                continue;
            }
            let Some(inv_h) = handle_of(&member.read_field("Inventory")?) else {
                continue;
            };
            let inv = own(inv_h);
            let n = inv.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
            let mut best: Option<(i32, String, i64)> = None;
            for j in 0..n {
                let Some(item_h) = handle_of(&inv.invoke("GetItem", &json!([j]))?) else {
                    continue;
                };
                let item = own(item_h);
                let amount = item
                    .invoke("GetAmount", &json!([]))
                    .ok()
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1);
                if amount >= ROB_MIN_AMOUNT
                    && best.as_ref().map(|b| amount > b.2).unwrap_or(true)
                {
                    let Some(proto_h) = handle_of(&item.invoke("GetPrototype", &json!([]))?)
                    else {
                        continue;
                    };
                    let iname = item
                        .invoke("GetDisplayNameString", &json!([]))
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .unwrap_or_else(|| "<goods>".into());
                    if let Some((old_p, ..)) = best.replace((proto_h, iname, amount)) {
                        drop(own(old_p));
                    }
                }
            }
            if let Some((proto_h, iname, amount)) = best {
                victim = Some((h, display_name(&com), proto_h, iname, amount));
                std::mem::forget(member);
                return Ok(false); // first rich traveler wins
            }
        }
        Ok(true)
    })?;
    let Some((victim_h, victim_community, proto_h, item_name, amount)) = victim else {
        drop(own(robber_h));
        return Ok(());
    };

    // The lead: our most menacing free member; Ambush staffs the
    // rest of the party itself (Enforcers and Guards first).
    let lead = with(robber_h, |com| pick_lead(com, &robber_ctype))?;
    let Some((lead_h, lead_name)) = lead else {
        drop(own(robber_h));
        drop(own(victim_h));
        drop(own(proto_h));
        return Ok(());
    };

    with(robber_h, |com| {
        com.invoke(
            "Ambush",
            &json!([
                { "handle": lead_h },
                { "handle": victim_h },
                NEVER_TELEPORT_SECS,
                4,
                { "handle": proto_h },
            ]),
        )
    })?;

    let franchise = voter_ids.len();
    MISSIONS.lock().push(Mission {
        robber_h,
        robber_id,
        robber_name: robber_name.clone(),
        victim_name: victim_community.clone(),
        lead_h,
        lead_name: lead_name.clone(),
        proto_h,
        eval_at: now + OUTCOME_DELAY_SECS,
        voter_ids,
    });
    drop(own(victim_h));

    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: rob -- {} ({}, {} of {} voters menacing, {:.2}) sends {} and a party to rob {} of {} x{}",
            robber_name, robber_ctype, votes, franchise, eff, lead_name, victim_community,
            item_name, amount,
        ),
    );
    Ok(())
}

fn pick_lead(
    com: &unityforge::mono::MonoObject,
    camp_ctype: &str,
) -> Result<Option<(i32, String)>, String> {
    let leader_id = handle_of(&com.read_field("Leader")?)
        .map(|h| own(h).read_field("Id").ok().and_then(|v| v.as_i64()).unwrap_or(-1));
    let mut lead: Option<(i32, String, f64)> = None;
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
            let conscious = member
                .invoke("get_IsConscious", &json!([]))
                .map(|v| v == json!(true))
                .unwrap_or(false);
            let squadded =
                handle_of(&member.invoke("GetSquad", &json!([])).unwrap_or(Json::Null)).is_some();
            let id = member.read_field("Id").ok().and_then(|v| v.as_i64()).unwrap_or(-1);
            if !alive || !human || !conscious || squadded || Some(id) == leader_id {
                continue;
            }
            let g = genome::individual(id, camp_ctype);
            let s = (g.get(Trait::Aggression) + g.get(Trait::Guile)) / 2.0;
            if lead.as_ref().map(|(_, _, bs)| s > *bs).unwrap_or(true) {
                let name = member
                    .invoke("GetDisplayNameString", &json!([]))
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| "<unnamed>".into());
                if let Some((old_h, ..)) = lead.replace((h, name, s)) {
                    drop(own(old_h));
                }
                std::mem::forget(member);
            }
        }
    }
    Ok(lead.map(|(h, name, _)| (h, name)))
}

// ---- judging -----------------------------------------------------------------

fn judge_missions(now: f32) {
    let mut missions = MISSIONS.lock();
    let mut i = 0;
    while i < missions.len() {
        if now < missions[i].eval_at {
            i += 1;
            continue;
        }
        let m = missions.remove(i);
        let lead_alive = with(m.lead_h, |l| l.invoke("get_AliveAndNotZombie", &json!([])))
            .map(|v| v == json!(true))
            .unwrap_or(false);
        let got_it = with(m.robber_h, |com| {
            com.invoke(
                "FindInventoryItemOfType",
                &json!([{ "handle": m.proto_h }, false]),
            )
        })
        .ok()
        .and_then(|v| handle_of(&v))
        .is_some();

        let (up, magnitude, verdict) = if !lead_alive {
            (false, 2.0, "the lead DIED for it; the camp sobers")
        } else if got_it {
            (true, 1.0, "the loot is in camp hands; menace paid")
        } else {
            (false, 0.5, "came home empty; a waste of menace")
        };
        for &v in &m.voter_ids {
            genome::reinforce_individual(v, Trait::Aggression, up, magnitude);
            genome::reinforce_individual(v, Trait::Guile, up, magnitude);
        }
        genome::reinforce(m.robber_id, Trait::Aggression, up, magnitude);
        genome::reinforce(m.robber_id, Trait::Guile, up, magnitude);
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: rob -- {}'s robbery of {}: {}",
                m.robber_name, m.victim_name, verdict,
            ),
        );
        drop(own(m.robber_h));
        drop(own(m.lead_h));
        drop(own(m.proto_h));
    }
}
