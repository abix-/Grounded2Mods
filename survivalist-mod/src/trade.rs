//! Trade: the peaceful act of the multidimensional repertoire
//! (docs/faction-war.md "Multidimensional factions").
//!
//! A well-fed camp whose franchise votes caution sends its most
//! careful free member as a one-person trade caravan to a
//! meaningfully hungrier neighbor: real food stacks loaded from
//! the home stores into the trader's hands, walked over as a real
//! 1-member Trade squad, handed into the host's storage, and paid
//! for with a non-food stack carried home. Barter, both sides
//! gain: the hungry camp eats, the surplus camp profits. Every
//! item moves by the game's own Take/Add transfer; the nutrition
//! ledger follows automatically because the game counts carried
//! and stored food alike.
//!
//! Vanilla AI-to-AI "trade" is cosmetic (trade squads only hang
//! out; goods move solely through the player trade UI). This is
//! the first real exchange between AI camps.
//!
//! Learning: the franchise voters learn defensiveness from the
//! outcome (a caravan home with payment reinforces the careful
//! way; a trader lost on the road weakens it): the third trait to
//! gain a live learning loop, after aggression (raids) and guile
//! (theft).

use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use unityforge::mono::{self, LogLevel};

use crate::common::{
    GoodsFilter, base_centre, carry_off_stored_goods, ctype, display_name, for_each_community,
    handle_of, list_len, own, with,
};
use crate::genome::{self, Trait};

/// Seconds between launch scans. Offset from the steal cadence so
/// the acts interleave rather than fire in lockstep.
const TRADE_SCAN_PERIOD_SECS: f32 = 150.0;

/// Seconds between mission-advance passes (arrival checks).
const MISSION_TICK_SECS: f32 = 5.0;

/// A voter favors trading if their own defensiveness clears this.
const TRADE_DEFENSIVENESS_FLOOR: f64 = 0.5;

/// The host must be this much hungrier than the seller: surplus
/// seeks need.
const TRADE_GAP: f64 = 0.2;

/// Food stacks a caravan carries out.
const TRADE_FOOD_STACKS: i64 = 2;

/// Non-food stacks taken home as payment.
const TRADE_PAY_STACKS: i64 = 1;

/// Within this squared tile distance of a building the caravan
/// has arrived; same bar for home.
const ARRIVE_DIST_SQ: f64 = 25.0;

/// A mission that has not resolved by then is abandoned.
const MISSION_TIMEOUT_SECS: f32 = 900.0;

/// At most this many caravans on the road map-wide.
const MAX_ACTIVE_MISSIONS: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Going,
    Returning,
}

/// An in-flight trade. The mission keeps its seller, host, and
/// trader handles alive (the launch scan's release pass skips
/// them) until cleanup releases all three.
struct Mission {
    seller_h: i32,
    seller_id: i64,
    seller_name: String,
    host_h: i32,
    host_name: String,
    trader_h: i32,
    trader_name: String,
    squad_id: i64,
    home: (i64, i64),
    stage: Stage,
    loaded: i64,
    delivered: i64,
    paid: i64,
    voter_ids: Vec<i64>,
    deadline: f32,
}

static MISSIONS: Mutex<Vec<Mission>> = Mutex::new(Vec::new());
static LAST_SCAN_BITS: AtomicU32 = AtomicU32::new(0);
static LAST_TICK_BITS: AtomicU32 = AtomicU32::new(0);

/// The active trade a faction is running, for survival_status.
pub fn active_target(faction_id: i64) -> Option<Json> {
    MISSIONS.lock().iter().find(|m| m.seller_id == faction_id).map(|m| {
        json!({
            "host": m.host_name,
            "trader": m.trader_name,
            "stage": match m.stage { Stage::Going => "going", Stage::Returning => "returning" },
        })
    })
}

pub fn tick(now: f32) {
    let last_tick = f32::from_bits(LAST_TICK_BITS.load(Ordering::Relaxed));
    if now - last_tick >= MISSION_TICK_SECS {
        LAST_TICK_BITS.store(now.to_bits(), Ordering::Relaxed);
        advance_missions(now);
    }
    let last_scan = f32::from_bits(LAST_SCAN_BITS.load(Ordering::Relaxed));
    if now - last_scan >= TRADE_SCAN_PERIOD_SECS {
        LAST_SCAN_BITS.store(now.to_bits(), Ordering::Relaxed);
        if let Err(e) = launch_scan(now) {
            if !e.contains("not found") {
                mono::log(LogLevel::Warn, &format!("survivalist-mod: trade scan failed: {e}"));
            }
        }
    }
}

// ---- launching ---------------------------------------------------------------

struct Camp {
    handle: i32,
    id: i64,
    name: String,
    ctype: String,
    nutrition: f64,
    centre: (i64, i64),
    votes: i64,
    franchise: i64,
    effective_defensiveness: f64,
    voter_ids: Vec<i64>,
    eligible_seller: bool,
}

fn launch_scan(now: f32) -> Result<(), String> {
    if MISSIONS.lock().len() >= MAX_ACTIVE_MISSIONS {
        return Ok(());
    }

    let mut camps: Vec<Camp> = Vec::new();
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
        if members == 0 {
            return Ok(true);
        }
        let id = com.read_field("Id")?.as_i64().unwrap_or(-1);
        let nutrition = com
            .invoke("CalcCommunityNutritionLevel", &json!([0.0]))?
            .as_f64()
            .unwrap_or(1.0);
        let Some(centre) = base_centre(&com) else {
            return Ok(true);
        };
        let at_war = handle_of(&com.read_field("InvasionTarget")?).is_some();
        let threats = list_len(&com, "Threats");
        let can_sell = members >= 3 && !at_war && threats == 0;

        let mut votes = 0i64;
        let mut franchise = 0i64;
        let mut sum_def = 0.0f64;
        let mut voter_ids = Vec::new();
        if can_sell {
            let looter = t == "Looter";
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
                    let d = genome::individual(char_id, &t).get(Trait::Defensiveness);
                    franchise += 1;
                    sum_def += d;
                    if d >= TRADE_DEFENSIVENESS_FLOOR {
                        votes += 1;
                    }
                    voter_ids.push(char_id);
                }
            }
        }
        camps.push(Camp {
            handle: com.handle().0,
            id,
            name: display_name(&com),
            ctype: t,
            nutrition,
            centre,
            votes,
            franchise,
            effective_defensiveness: if franchise > 0 { sum_def / franchise as f64 } else { 0.0 },
            voter_ids,
            eligible_seller: can_sell,
        });
        std::mem::forget(com);
        Ok(true)
    })?;

    // The seller: voted-yes camps not already trading, most
    // careful franchise first.
    let active: Vec<i64> = MISSIONS.lock().iter().map(|m| m.seller_id).collect();
    let mut sellers: Vec<&Camp> = camps
        .iter()
        .filter(|c| {
            c.eligible_seller
                && c.franchise > 0
                && c.votes * 2 > c.franchise
                && !active.contains(&c.id)
        })
        .collect();
    sellers.sort_by(|a, b| {
        b.effective_defensiveness
            .partial_cmp(&a.effective_defensiveness)
            .unwrap()
    });

    for camp in sellers {
        // The host: nearest camp meaningfully hungrier, not an
        // enemy (allies welcome; trade is how friends stay fed).
        let mut best: Option<(&Camp, i64)> = None;
        for t in &camps {
            if t.handle == camp.handle || t.nutrition + TRADE_GAP > camp.nutrition {
                continue;
            }
            let rel = with(camp.handle, |c| {
                c.invoke("GetRelationship", &json!([{ "handle": t.handle }]))
            })
            .unwrap_or(json!("?"));
            if rel == json!("Hostile") {
                continue;
            }
            let d = (t.centre.0 - camp.centre.0).pow(2) + (t.centre.1 - camp.centre.1).pow(2);
            if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((t, d));
            }
        }
        let Some((host, _)) = best else { continue };

        if let Err(e) = launch(camp, host, now) {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: trade launch failed for {}: {e}", camp.name),
            );
        }
        break; // one new caravan per scan
    }

    // Release the snapshot handles, EXCEPT the ones a live mission
    // (including one just launched) still owns.
    let kept: Vec<i32> = {
        let ms = MISSIONS.lock();
        ms.iter().flat_map(|m| [m.seller_h, m.host_h]).collect()
    };
    for c in &camps {
        if !kept.contains(&c.handle) {
            drop(own(c.handle));
        }
    }
    Ok(())
}

fn launch(camp: &Camp, host: &Camp, now: f32) -> Result<(), String> {
    with(camp.handle, |com| {
        // The trader: the most careful free member (highest
        // defensiveness, conscious, not the leader, not squadded).
        let leader_id = handle_of(&com.read_field("Leader")?)
            .map(|h| own(h).read_field("Id").ok().and_then(|v| v.as_i64()).unwrap_or(-1));
        let mut trader: Option<(i32, String, f64)> = None;
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
                    handle_of(&member.invoke("GetSquad", &json!([])).unwrap_or(Json::Null))
                        .is_some();
                let id = member.read_field("Id").ok().and_then(|v| v.as_i64()).unwrap_or(-1);
                if !alive || !human || !conscious || squadded || Some(id) == leader_id {
                    continue;
                }
                let d = genome::individual(id, &camp.ctype).get(Trait::Defensiveness);
                if trader.as_ref().map(|(_, _, bd)| d > *bd).unwrap_or(true) {
                    let name = member
                        .invoke("GetDisplayNameString", &json!([]))
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .unwrap_or_else(|| "<unnamed>".into());
                    if let Some((old_h, ..)) = trader.replace((h, name, d)) {
                        drop(own(old_h));
                    }
                    std::mem::forget(member);
                }
            }
        }
        let Some((trader_h, trader_name, _)) = trader else {
            return Ok(()); // nobody free to send
        };

        // Load the caravan from the home stores BEFORE leaving.
        // Nothing to sell = no trip (and no squad to clean up).
        let loaded = carry_off_stored_goods(com, &[trader_h], TRADE_FOOD_STACKS, GoodsFilter::Food)?;
        if loaded == 0 {
            drop(own(trader_h));
            return Ok(());
        }

        // On the road as a real 1-member Trade squad (the game's
        // own machinery; pathing, gates, and reactions all vanilla).
        let squad_h =
            handle_of(&com.invoke("AddSquad", &json!(["Trade", 0]))?).ok_or("AddSquad gave no squad")?;
        let squad = own(squad_h);
        com.invoke(
            "AddToSquad",
            &json!([{ "handle": trader_h }, { "handle": squad_h }]),
        )?;
        let dest = json!({"x": host.centre.0, "y": host.centre.1});
        squad.write_field("GoalTile", &dest)?;
        com.invoke(
            "SetSquadAction",
            &json!([{ "handle": squad_h }, "GoTo", 0, dest, null, false]),
        )?;
        let squad_id = squad.read_field("Id")?.as_i64().unwrap_or(-1);
        drop(squad);

        MISSIONS.lock().push(Mission {
            seller_h: camp.handle,
            seller_id: camp.id,
            seller_name: camp.name.clone(),
            host_h: host.handle,
            host_name: host.name.clone(),
            trader_h,
            trader_name: trader_name.clone(),
            squad_id,
            home: camp.centre,
            stage: Stage::Going,
            loaded,
            delivered: 0,
            paid: 0,
            voter_ids: camp.voter_ids.clone(),
            deadline: now + MISSION_TIMEOUT_SECS,
        });

        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: trade -- {} ({}, fed {:.2}, {} of {} voters careful) sends {} with {} food stack(s) to hungry {} ({:.2})",
                camp.name, camp.ctype, camp.nutrition, camp.votes, camp.franchise, trader_name, loaded, host.name, host.nutrition,
            ),
        );
        Ok(())
    })
}

// ---- advancing ---------------------------------------------------------------

fn advance_missions(now: f32) {
    let mut missions = MISSIONS.lock();
    let mut i = 0;
    while i < missions.len() {
        let done = advance(&mut missions[i], now).unwrap_or(true);
        if done {
            let m = missions.remove(i);
            with(m.seller_h, |com| {
                if let Ok(sq) = com.invoke("GetSquad", &json!([m.squad_id])) {
                    if let Some(sq_h) = handle_of(&sq) {
                        let _ = com.invoke("RemoveSquad", &json!([{ "handle": sq_h }]));
                    }
                }
            });
            drop(own(m.seller_h));
            drop(own(m.host_h));
            drop(own(m.trader_h));
        } else {
            i += 1;
        }
    }
}

/// One mission step. Ok(true) = mission over, clean up.
fn advance(m: &mut Mission, now: f32) -> Result<bool, String> {
    let alive = with(m.trader_h, |t| t.invoke("get_AliveAndNotZombie", &json!([])))? == json!(true);
    if !alive {
        // A caravan lost on the road: caution failed to keep them
        // safe, and the goods died with the trader.
        for &v in &m.voter_ids {
            genome::reinforce_individual(v, Trait::Defensiveness, false, 2.0);
        }
        genome::reinforce(m.seller_id, Trait::Defensiveness, false, 2.0);
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: trade -- {}'s trader {} DIED on the road to {}; the camp loses faith in the careful way",
                m.seller_name, m.trader_name, m.host_name,
            ),
        );
        return Ok(true);
    }
    if now >= m.deadline {
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: trade -- {}'s caravan to {} fizzled (timeout); {} recalled",
                m.seller_name, m.host_name, m.trader_name,
            ),
        );
        return Ok(true);
    }

    let tile = with(m.trader_h, |t| t.invoke("get_Tile", &json!([])))?;
    match m.stage {
        Stage::Going => {
            let host_alive = with(m.host_h, |h| {
                h.invoke("HasAnyLivingNonZombieMembers", &json!([]))
            })
            .map(|v| v == json!(true))
            .unwrap_or(false);
            if !host_alive {
                return Ok(true); // the market died; go home
            }
            let d = with(m.host_h, |h| {
                h.invoke("GetDistSqToNearestBuilding", &json!([tile.clone()]))
            })?
            .as_f64()
            .unwrap_or(f64::MAX);
            if d > ARRIVE_DIST_SQ {
                return Ok(false);
            }
            // The exchange, all real hands and real containers:
            // carried food into the host's stores, a non-food
            // stack back as payment.
            m.delivered = deliver_carried_food(m.trader_h, m.host_h, m.loaded)?;
            m.paid = with(m.host_h, |h| {
                carry_off_stored_goods(h, &[m.trader_h], TRADE_PAY_STACKS, GoodsFilter::NonFood)
            })?;
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: trade -- {} delivers {} food stack(s) to {} and takes {} stack(s) home as payment",
                    m.trader_name, m.delivered, m.host_name, m.paid,
                ),
            );
            let home = json!({"x": m.home.0, "y": m.home.1});
            with(m.seller_h, |com| -> Result<(), String> {
                if let Ok(sq) = com.invoke("GetSquad", &json!([m.squad_id])) {
                    if let Some(sq_h) = handle_of(&sq) {
                        let squad = own(sq_h);
                        squad.write_field("GoalTile", &home)?;
                        com.invoke(
                            "SetSquadAction",
                            &json!([{ "handle": sq_h }, "GoTo", 0, home, null, false]),
                        )?;
                    }
                }
                Ok(())
            })?;
            m.stage = Stage::Returning;
            Ok(false)
        }
        Stage::Returning => {
            let d = with(m.seller_h, |com| {
                com.invoke("GetDistSqToNearestBuilding", &json!([tile]))
            })?
            .as_f64()
            .unwrap_or(f64::MAX);
            if d > ARRIVE_DIST_SQ {
                return Ok(false);
            }
            if m.delivered > 0 {
                // Home with the deal done: the careful way pays.
                for &v in &m.voter_ids {
                    genome::reinforce_individual(v, Trait::Defensiveness, true, 1.0);
                }
                genome::reinforce(m.seller_id, Trait::Defensiveness, true, 1.0);
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: trade -- {} comes home to {}; the trade with {} paid, and the camp trusts the careful way more",
                        m.trader_name, m.seller_name, m.host_name,
                    ),
                );
            }
            Ok(true)
        }
    }
}

/// Move up to `max` FOOD stacks from the trader's carried
/// inventory into the first host building that will hold them:
/// the delivery half of the barter, on the same Take/Add calls as
/// everything else.
fn deliver_carried_food(trader_h: i32, host_h: i32, max: i64) -> Result<i64, String> {
    // The receiving shelf: the host's first building with an
    // inventory container.
    let store: Option<(i32, i32)> = with(host_h, |host| {
        let b_h = handle_of(&host.read_field("Buildings").ok()?)?;
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

    let trader_inv_h = with(trader_h, |t| handle_of(&t.read_field("Inventory")?).ok_or("trader has no inventory".to_string()))?;
    let trader_inv = own(trader_inv_h);
    let store_inv = own(store_inv_h);
    let mut delivered = 0i64;
    while delivered < max {
        let count = trader_inv
            .invoke("get_Count", &json!([]))
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let mut pick: Option<(i32, i64)> = None;
        for i in 0..count {
            let Some(item_h) = handle_of(&trader_inv.invoke("GetItem", &json!([i]))?) else {
                continue;
            };
            let item = own(item_h);
            let food = item
                .invoke("GetNutrition", &json!([]))
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                > 0.0;
            let amount = item
                .invoke("GetAmount", &json!([]))
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(1);
            if food {
                std::mem::forget(item);
                pick = Some((item_h, amount));
                break;
            }
        }
        let Some((item_h, amount)) = pick else { break };
        let taken = trader_inv.invoke(
            "Take",
            &json!([{ "handle": trader_h }, { "handle": item_h }, amount]),
        )?;
        let Some(taken_h) = handle_of(&taken) else { break };
        store_inv.invoke(
            "Add",
            &json!([{ "handle": store_bh }, { "handle": taken_h }]),
        )?;
        delivered += 1;
    }
    drop(trader_inv);
    drop(store_inv);
    drop(own(store_bh));
    Ok(delivered)
}
