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

use std::sync::atomic::AtomicU32;

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::genome::{Ballot, majority};
use modforge::mission::{self, Stage, Step};
use unityforge::mono::{self, LogLevel};

use crate::common::{
    GoodsFilter, base_centre, carry_off_stored_goods, ctype, display_name, dist_sq_to_building,
    for_each_community, handle_of, is_npc_alive, own, remove_squad_and_drop, send_squad_home, with,
};
use crate::genome;

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
const MISSION_TIMEOUT_SECS: f32 = 1800.0;

/// At most this many caravans on the road map-wide.
const MAX_ACTIVE_MISSIONS: usize = 3;

/// An in-flight trade. The mission keeps its seller, host, and
/// trader handles alive (the launch scan's release pass skips
/// them) until cleanup releases all three.
struct Mission {
    seller_h: i32,
    seller_id: i64,
    seller_name: String,
    host_h: i32,
    host_name: String,
    host_is_player: bool,
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

/// A camp's voted-but-could-not-launch line logs at most once per
/// this window; a gutted camp re-voting every scan was spamming
/// the log (Smiley Crow Militia, live 2026-07-05).
const FAILED_LAUNCH_LOG_COOLDOWN_SECS: f32 = 1800.0;
static FAILED_LAUNCH_LOGGED: Mutex<Vec<(i64, f32)>> = Mutex::new(Vec::new());

/// Rate-limit repeated explanations when a faction cannot form a caravan.
/// Stays here because it applies Survivalist's trade missions rules through the game's classes, fields, content, and actions.
fn should_log_failed_launch(faction_id: i64, now: f32) -> bool {
    let mut seen = FAILED_LAUNCH_LOGGED.lock();
    if let Some((_, at)) = seen.iter_mut().find(|(id, _)| *id == faction_id) {
        if now - *at < FAILED_LAUNCH_LOG_COOLDOWN_SECS {
            return false;
        }
        *at = now;
        return true;
    }
    seen.push((faction_id, now));
    true
}

/// The active trade a faction is running, for survival_status.
/// Stays here because it applies Survivalist's trade missions rules through the game's classes, fields, content, and actions.
pub fn active_target(faction_id: i64) -> Option<Json> {
    MISSIONS
        .lock()
        .iter()
        .find(|m| m.seller_id == faction_id)
        .map(|m| {
            json!({
                "host": m.host_name,
                "trader": m.trader_name,
                "stage": match m.stage { Stage::Going => "going", Stage::Returning => "returning" },
            })
        })
}

/// Advance this system when its scheduled game update is due.
/// Stays here because it applies Survivalist's trade missions rules through the game's classes, fields, content, and actions.
pub fn tick(now: f32) {
    if mission::should_tick(now, MISSION_TICK_SECS, &LAST_TICK_BITS) {
        mission::advance_all(&MISSIONS, now, |m, e| {
            mono::log(
                LogLevel::Warn,
                &format!(
                    "survivalist-mod: trade: mission for {} aborted: {e}",
                    m.seller_name
                ),
            );
        });
    }
    if mission::should_tick(now, TRADE_SCAN_PERIOD_SECS, &LAST_SCAN_BITS) {
        if let Err(e) = launch_scan(now) {
            if !e.contains("not found") {
                mono::log(
                    LogLevel::Warn,
                    &format!("survivalist-mod: trade scan failed: {e}"),
                );
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

/// Find one faction ready to start trade missions.
/// Stays here because it applies Survivalist's trade missions rules through the game's classes, fields, content, and actions.
fn launch_scan(now: f32) -> Result<(), String> {
    if MISSIONS.lock().len() >= MAX_ACTIVE_MISSIONS {
        return Ok(());
    }

    let mut camps: Vec<Camp> = Vec::new();
    for_each_community(|com| {
        let t = ctype(&com);
        // The player's camp is a HOST-ONLY candidate: caravans may
        // arrive at the player's gate, but the player camp never
        // sells (docs/faction-war.md "The player joins the
        // ecosystem").
        if t == "Player" {
            let members = com
                .invoke("GetLivingNonZombieMemberCount", &json!([]))?
                .as_i64()
                .unwrap_or(0);
            let Some(centre) = base_centre(&com) else {
                return Ok(true);
            };
            if members == 0 {
                return Ok(true);
            }
            let nutrition = com
                .invoke("CalcCommunityNutritionLevel", &json!([0.0]))?
                .as_f64()
                .unwrap_or(1.0);
            camps.push(Camp {
                handle: com.handle().0,
                id: com.read_field("Id")?.as_i64().unwrap_or(-1),
                name: display_name(&com),
                ctype: t,
                nutrition,
                centre,
                votes: 0,
                franchise: 0,
                effective_defensiveness: 0.0,
                voter_ids: Vec::new(),
                eligible_seller: false,
            });
            std::mem::forget(com);
            return Ok(true);
        }
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
        let threats = com.field_list_len("Threats");
        let can_sell = members >= 3 && !at_war && threats == 0;

        let mut ballot = Ballot::new(TRADE_DEFENSIVENESS_FLOOR);
        if can_sell {
            let looter = t == "Looter";
            if let Some(m_h) = handle_of(&com.read_field("Members")?) {
                let mlist = own(m_h);
                let count = mlist.list_len_or_zero()?;
                for i in 0..count {
                    let Some(h) = mlist.list_handle(i)? else {
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
                    let d = genome::individual(char_id, &t)[genome::DEFENSIVENESS];
                    ballot.cast(char_id, d);
                }
            }
        }
        let effective_defensiveness = ballot.mean_score();
        camps.push(Camp {
            handle: com.handle().0,
            id,
            name: display_name(&com),
            ctype: t,
            nutrition,
            centre,
            votes: ballot.votes_for,
            franchise: ballot.franchise,
            effective_defensiveness,
            voter_ids: ballot.voter_ids,
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
        .filter(|c| c.eligible_seller && majority(c.votes, c.franchise) && !active.contains(&c.id))
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
                &format!(
                    "survivalist-mod: trade launch failed for {}: {e}",
                    camp.name
                ),
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

/// Start trade missions using real survivors, supplies, and game movement.
/// Stays here because it applies Survivalist's trade missions rules through the game's classes, fields, content, and actions.
fn launch(camp: &Camp, host: &Camp, now: f32) -> Result<(), String> {
    with(camp.handle, |com| {
        // The trader: the most careful free member (highest
        // defensiveness, conscious, not the leader, not squadded).
        let leader_id = handle_of(&com.read_field("Leader")?).map(|h| {
            own(h)
                .read_field("Id")
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(-1)
        });
        let mut trader: Option<(i32, i64, String, f64)> = None;
        if let Some(m_h) = handle_of(&com.read_field("Members")?) {
            let mlist = own(m_h);
            let count = mlist.list_len_or_zero()?;
            for i in 0..count {
                let Some(h) = mlist.list_handle(i)? else {
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
                let id = member
                    .read_field("Id")
                    .ok()
                    .and_then(|v| v.as_i64())
                    .unwrap_or(-1);
                if !alive || !human || !conscious || squadded || Some(id) == leader_id {
                    continue;
                }
                let d = genome::individual(id, &camp.ctype)[genome::DEFENSIVENESS];
                if trader.as_ref().map(|(_, _, _, bd)| d > *bd).unwrap_or(true) {
                    let name = member
                        .invoke("GetDisplayNameString", &json!([]))
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .unwrap_or_else(|| "<unnamed>".into());
                    if let Some((old_h, ..)) = trader.replace((h, id, name, d)) {
                        drop(own(old_h));
                    }
                    std::mem::forget(member);
                }
            }
        }
        let Some((trader_h, trader_id, trader_name, _)) = trader else {
            if should_log_failed_launch(camp.id, now) {
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: trade -- {} voted to trade with {} but has no free member to send",
                        camp.name, host.name,
                    ),
                );
            }
            return Ok(());
        };

        // Load the caravan BEFORE leaving: building stores first,
        // then campmates' carried surplus (real hand-offs at home;
        // every donor keeps a stack for themselves). Camps keep
        // most food planted and carried, not warehoused, so the
        // member top-up is usually the real source.
        let mut loaded = carry_off_stored_goods(
            com,
            &[trader_h],
            TRADE_FOOD_STACKS,
            GoodsFilter::Food,
            false,
        )?;
        if loaded < TRADE_FOOD_STACKS {
            loaded += load_food_from_members(com, trader_h, trader_id, TRADE_FOOD_STACKS - loaded)?;
        }
        if loaded == 0 {
            if should_log_failed_launch(camp.id, now) {
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: trade -- {} voted to trade with {} but has no spare food to load",
                        camp.name, host.name,
                    ),
                );
            }
            drop(own(trader_h));
            return Ok(());
        }

        // On the road as a real 1-member Trade squad (the game's
        // own machinery; pathing, gates, and reactions all vanilla).
        let squad_h = handle_of(&com.invoke("AddSquad", &json!(["Trade", 0]))?)
            .ok_or("AddSquad gave no squad")?;
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
            host_is_player: host.ctype == "Player",
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
                camp.name,
                camp.ctype,
                camp.nutrition,
                camp.votes,
                camp.franchise,
                trader_name,
                loaded,
                host.name,
                host.nutrition,
            ),
        );
        Ok(())
    })
}

// ---- advancing ---------------------------------------------------------------

// ---- Mission trait ---------------------------------------------------------

impl mission::Mission for Mission {
    modforge::mission_accessors!();

    /// Check whether the mission agent can continue.
    /// Stays here because it applies Survivalist's trade missions rules through the game's classes, fields, content, and actions.
    fn is_agent_alive(&self) -> Result<bool, String> {
        let alive = is_npc_alive(self.trader_h)?;
        if !alive {
            genome::reinforce_collective(
                self.seller_id,
                &self.voter_ids,
                &[genome::DEFENSIVENESS],
                false,
                2.0,
            );
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: trade: {}'s trader {} died on the road to {}; the camp loses faith in the careful way",
                    self.seller_name, self.trader_name, self.host_name,
                ),
            );
        }
        Ok(alive)
    }

    /// Resolve what happens when the mission reaches its destination.
    /// Stays here because it applies Survivalist's trade missions rules through the game's classes, fields, content, and actions.
    fn on_going(&mut self, _now: f32) -> Result<Step, String> {
        let host_alive = with(self.host_h, |h| {
            h.invoke("HasAnyLivingNonZombieMembers", &json!([]))
        })
        .map(|v| v == json!(true))
        .unwrap_or(false);
        if !host_alive {
            return Ok(Step::Complete);
        }
        if dist_sq_to_building(self.trader_h, self.host_h)? > ARRIVE_DIST_SQ {
            return Ok(Step::Continue);
        }
        self.delivered = deliver_carried_food(self.trader_h, self.host_h, self.loaded)?;
        self.paid = with(self.host_h, |h| {
            carry_off_stored_goods(
                h,
                &[self.trader_h],
                TRADE_PAY_STACKS,
                GoodsFilter::NonFood,
                false,
            )
        })?;
        if self.host_is_player && self.delivered > 0 {
            crate::chronicle::post(&format!(
                "{} has sent {} to your gate with food",
                self.seller_name, self.trader_name
            ));
        }
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: trade: {} delivers {} food stack(s) to {} and takes {} stack(s) home as payment",
                self.trader_name, self.delivered, self.host_name, self.paid,
            ),
        );
        send_squad_home(self.seller_h, self.squad_id, self.home)?;
        Ok(Step::Transition)
    }

    /// Resolve what happens when the mission agent returns home.
    /// Stays here because it applies Survivalist's trade missions rules through the game's classes, fields, content, and actions.
    fn on_returning(&mut self, _now: f32) -> Result<Step, String> {
        if dist_sq_to_building(self.trader_h, self.seller_h)? > ARRIVE_DIST_SQ {
            return Ok(Step::Continue);
        }
        if self.delivered > 0 {
            genome::reinforce_collective(
                self.seller_id,
                &self.voter_ids,
                &[genome::DEFENSIVENESS],
                true,
                1.0,
            );
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: trade: {} comes home to {}; the trade with {} paid, and the camp trusts the careful way more",
                    self.trader_name, self.seller_name, self.host_name,
                ),
            );
        }
        Ok(Step::Complete)
    }

    /// Release the mission squad and managed handles when the mission ends.
    /// Stays here because it applies Survivalist's trade missions rules through the game's classes, fields, content, and actions.
    fn cleanup(self) {
        remove_squad_and_drop(
            self.seller_h,
            self.squad_id,
            &[self.seller_h, self.host_h, self.trader_h],
        );
    }

    /// Describe the active work for status output.
    /// Stays here because it applies Survivalist's trade missions rules through the game's classes, fields, content, and actions.
    fn label(&self) -> String {
        format!("{} trading with {}", self.seller_name, self.host_name)
    }
}

/// Top up the caravan from campmates' carried food: a real
/// hand-off at home via the same Take/Add transfer. Each donor
/// keeps at least one food stack for themselves.
/// Stays here because it applies Survivalist's trade missions rules through the game's classes, fields, content, and actions.
fn load_food_from_members(
    com: &unityforge::mono::MonoObject,
    trader_h: i32,
    trader_id: i64,
    need: i64,
) -> Result<i64, String> {
    let mut gained = 0i64;
    let Some(m_h) = handle_of(&com.read_field("Members")?) else {
        return Ok(0);
    };
    let mlist = own(m_h);
    let count = mlist.list_len_or_zero()?;
    'members: for i in 0..count {
        if gained >= need {
            break;
        }
        let Some(h) = mlist.list_handle(i)? else {
            continue;
        };
        let member = own(h);
        // Handles from separate bridge calls never match; the
        // trader is skipped by character Id.
        let id = member
            .read_field("Id")
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(-1);
        if id == trader_id
            || member
                .invoke("get_AliveAndNotZombie", &json!([]))
                .map(|v| v != json!(true))
                .unwrap_or(true)
        {
            continue;
        }
        let Some(inv_h) = handle_of(&member.read_field("Inventory")?) else {
            continue;
        };
        let inv = own(inv_h);
        loop {
            let n = inv.list_len().unwrap_or(0);
            // Count the donor's food stacks and find one to give.
            let mut food_stacks = 0i64;
            let mut pick: Option<(i32, i64)> = None;
            for j in 0..n {
                let Some(item_h) = handle_of(&inv.invoke("GetItem", &json!([j]))?) else {
                    continue;
                };
                let item = own(item_h);
                let food = item
                    .invoke("GetNutrition", &json!([]))
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    > 0.0;
                if food {
                    food_stacks += 1;
                    if pick.is_none() {
                        let amount = item
                            .invoke("GetAmount", &json!([]))
                            .ok()
                            .and_then(|v| v.as_i64())
                            .unwrap_or(1);
                        std::mem::forget(item);
                        pick = Some((item_h, amount));
                    }
                }
            }
            // Leave the donor their last stack.
            let Some((item_h, amount)) = pick else {
                continue 'members;
            };
            if food_stacks <= 1 || gained >= need {
                continue 'members;
            }
            let taken = inv.invoke(
                "Take",
                &json!([{ "handle": h }, { "handle": item_h }, amount]),
            )?;
            let Some(taken_h) = handle_of(&taken) else {
                continue 'members;
            };
            let trader = own(trader_h);
            let _ = trader.invoke(
                "Add",
                &json!([{ "handle": trader_h }, { "handle": taken_h }]),
            );
            std::mem::forget(trader);
            gained += 1;
        }
    }
    Ok(gained)
}

/// Move up to `max` FOOD stacks from the trader's carried
/// inventory into the first host building that will hold them:
/// the delivery half of the barter, on the same Take/Add calls as
/// everything else.
/// Stays here because it applies Survivalist's trade missions rules through the game's classes, fields, content, and actions.
fn deliver_carried_food(trader_h: i32, host_h: i32, max: i64) -> Result<i64, String> {
    // The receiving shelf: the host's first building with an
    // inventory container.
    let store: Option<(i32, i32)> = with(host_h, |host| {
        let b_h = handle_of(&host.read_field("Buildings").ok()?)?;
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

    let trader_inv_h = with(trader_h, |t| {
        handle_of(&t.read_field("Inventory")?).ok_or("trader has no inventory".to_string())
    })?;
    let trader_inv = own(trader_inv_h);
    let store_inv = own(store_inv_h);
    let mut delivered = 0i64;
    while delivered < max {
        let count = trader_inv.list_len().unwrap_or(0);
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
        let Some(taken_h) = handle_of(&taken) else {
            break;
        };
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
