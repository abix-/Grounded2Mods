//! Traveling vendor: a storyteller event, the opportunity kind
//! (docs/status.md "Storyteller / director"; the event menu).
//!
//! When the director fires it, a well-stocked camp sends a member
//! out as a traveling merchant: real wares loaded from the home
//! stores into the trader's hands, walked over as a real 1-member
//! Trade squad (the same movement the trade act uses), swapped at
//! the destination, and the payment carried home. It favors the
//! player's gate but will visit another camp too. Every item moves
//! by the game's own Take/Add transfer; nothing is conjured.
//!
//! It deals in NON-FOOD wares both ways, so it never disturbs the
//! food/nutrition ledger, and it collects payment BEFORE handing
//! over the goods, so the payment pass can never grab back the
//! wares just delivered.
//!
//! Unlike the trade act (a camp's own defensiveness vote sends food
//! to a hungry neighbor), the vendor is DIRECTOR-paced drama: it
//! appears on Randy's irregular cadence, not because a camp decided
//! to. The horde is the pressure the director can bring; the vendor
//! is the hope.

use std::sync::atomic::AtomicU32;

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::mission::{self, Stage, Step};
use modforge::unknown::rng;
use unityforge::mono::{self, LogLevel, MonoObject};

use crate::common::{
    GoodsFilter, base_centre, carry_off_stored_goods, ctype, display_name, dist_sq_to_building,
    for_each_community, handle_of, is_npc_alive, own, remove_squad_and_drop, send_squad_home, with,
};
use crate::storyteller::{Outcome, Rule};

/// The vendor as a storyteller rule; the director paces it.
pub const RULE: Rule = Rule {
    name: "vendor",
    weight: 1,
    run,
};

/// Seconds between mission-advance passes (arrival checks).
const MISSION_TICK_SECS: f32 = 5.0;

/// A camp needs this many living members to spare one as a trader.
const VENDOR_MIN_MEMBERS: i64 = 3;

/// Non-food ware stacks the vendor carries out to sell.
const VENDOR_GOODS_STACKS: i64 = 2;

/// Non-food stacks taken as payment and carried home.
const VENDOR_PAY_STACKS: i64 = 1;

/// Within this squared tile distance of a building the caravan has
/// arrived; same bar for home.
const ARRIVE_DIST_SQ: f64 = 25.0;

/// A mission that has not resolved by then is abandoned.
const MISSION_TIMEOUT_SECS: f32 = 1800.0;

/// At most this many vendors on the road map-wide.
const MAX_VENDORS: usize = 2;

/// An in-flight vendor. The mission keeps its source, target, and
/// trader handles alive until cleanup releases all three.
struct Mission {
    source_h: i32,
    source_id: i64,
    source_name: String,
    target_h: i32,
    target_name: String,
    target_is_player: bool,
    trader_h: i32,
    trader_name: String,
    squad_id: i64,
    home: (i64, i64),
    stage: Stage,
    brought: i64,
    sold: i64,
    paid: i64,
    deadline: f32,
}

static MISSIONS: Mutex<Vec<Mission>> = Mutex::new(Vec::new());
static LAST_TICK_BITS: AtomicU32 = AtomicU32::new(0);

/// How many vendors are on the road, for the storyteller status
/// readout.
/// Stays here because it applies Survivalist's traveling vendors rules through the game's classes, fields, content, and actions.
pub fn active_count() -> usize {
    MISSIONS.lock().len()
}

/// Advance in-flight vendors. The director launches them (via RULE);
/// this walks them through arrival, the swap, and the trip home.
/// Stays here because it applies Survivalist's traveling vendors rules through the game's classes, fields, content, and actions.
pub fn tick(now: f32) {
    if mission::should_tick(now, MISSION_TICK_SECS, &LAST_TICK_BITS) {
        mission::advance_all(&MISSIONS, now, |m, e| {
            mono::log(
                LogLevel::Warn,
                &format!(
                    "survivalist-mod: vendor: mission from {} aborted: {e}",
                    m.source_name,
                ),
            );
        });
    }
}

// ---- launching (the storyteller rule) --------------------------------------

struct Camp {
    handle: i32,
    id: i64,
    name: String,
    is_player: bool,
    centre: (i64, i64),
    eligible_source: bool,
}

/// Choose and start the next eligible traveling vendors event.
/// Stays here because it applies Survivalist's traveling vendors rules through the game's classes, fields, content, and actions.
fn run(now: f32) -> Result<Outcome, String> {
    if MISSIONS.lock().len() >= MAX_VENDORS {
        return Ok(Outcome::Passed);
    }

    let mut camps: Vec<Camp> = Vec::new();
    for_each_community(|com| {
        let t = ctype(&com);
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
        let Some(centre) = base_centre(&com) else {
            return Ok(true);
        };
        let at_war = handle_of(&com.read_field("InvasionTarget")?).is_some();
        let threats = com.field_list_len("Threats");
        let eligible_source =
            !is_player && members >= VENDOR_MIN_MEMBERS && !at_war && threats == 0;
        camps.push(Camp {
            handle: com.handle().0,
            id: com.read_field("Id")?.as_i64().unwrap_or(-1),
            name: display_name(&com),
            is_player,
            centre,
            eligible_source,
        });
        std::mem::forget(com);
        Ok(true)
    })?;

    let result = try_launch(&camps, now);

    // Release snapshot handles except those a live mission owns.
    let kept: Vec<i32> = {
        let ms = MISSIONS.lock();
        ms.iter().flat_map(|m| [m.source_h, m.target_h]).collect()
    };
    for c in &camps {
        if !kept.contains(&c.handle) {
            drop(own(c.handle));
        }
    }
    result
}

/// Find a source and destination able to support traveling vendors.
/// Stays here because it applies Survivalist's traveling vendors rules through the game's classes, fields, content, and actions.
fn try_launch(camps: &[Camp], now: f32) -> Result<Outcome, String> {
    let sources: Vec<&Camp> = camps
        .iter()
        .filter(|c| c.eligible_source && !mission_active_source(c.id))
        .collect();
    if sources.is_empty() {
        return Ok(Outcome::Passed);
    }
    let source = sources[rng(now, 0, sources.len() as u64) as usize];

    // Valid targets: not the source, not hostile to it.
    let mut targets: Vec<&Camp> = Vec::new();
    for c in camps {
        if c.handle == source.handle {
            continue;
        }
        let rel = with(source.handle, |s| {
            s.invoke("GetRelationship", &json!([{ "handle": c.handle }]))
        })
        .unwrap_or(json!("?"));
        if rel == json!("Hostile") {
            continue;
        }
        targets.push(c);
    }
    if targets.is_empty() {
        return Ok(Outcome::Passed);
    }
    // Favor the player's gate when it is a valid target.
    let player = targets.iter().copied().find(|c| c.is_player);
    let target = match player {
        Some(p) if rng(now, 1, 2) as usize == 0 => p,
        _ => targets[rng(now, 2, targets.len() as u64) as usize],
    };

    launch(source, target, now)
}

/// Start traveling vendors using real survivors, supplies, and game movement.
/// Stays here because it applies Survivalist's traveling vendors rules through the game's classes, fields, content, and actions.
fn launch(source: &Camp, target: &Camp, now: f32) -> Result<Outcome, String> {
    with(source.handle, |com| {
        let Some((trader_h, trader_name)) = pick_free_member(com)? else {
            return Ok(Outcome::Passed);
        };

        // Load the wares BEFORE leaving, from the source's stores.
        let brought = carry_off_stored_goods(
            com,
            &[trader_h],
            VENDOR_GOODS_STACKS,
            GoodsFilter::NonFood,
            false,
        )?;
        if brought == 0 {
            drop(own(trader_h)); // nothing to sell; try again later
            return Ok(Outcome::Passed);
        }

        // On the road as a real 1-member Trade squad (the game's own
        // machinery; pathing, gates, and reactions all vanilla).
        let squad_h = handle_of(&com.invoke("AddSquad", &json!(["Trade", 0]))?)
            .ok_or("AddSquad gave no squad")?;
        let squad = own(squad_h);
        com.invoke(
            "AddToSquad",
            &json!([{ "handle": trader_h }, { "handle": squad_h }]),
        )?;
        let dest = json!({"x": target.centre.0, "y": target.centre.1});
        squad.write_field("GoalTile", &dest)?;
        com.invoke(
            "SetSquadAction",
            &json!([{ "handle": squad_h }, "GoTo", 0, dest, null, false]),
        )?;
        let squad_id = squad.read_field("Id")?.as_i64().unwrap_or(-1);
        drop(squad);

        MISSIONS.lock().push(Mission {
            source_h: source.handle,
            source_id: source.id,
            source_name: source.name.clone(),
            target_h: target.handle,
            target_name: target.name.clone(),
            target_is_player: target.is_player,
            trader_h,
            trader_name: trader_name.clone(),
            squad_id,
            home: source.centre,
            stage: Stage::Going,
            brought,
            sold: 0,
            paid: 0,
            deadline: now + MISSION_TIMEOUT_SECS,
        });

        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: vendor -- {} sends {} with {} ware(s) to {}",
                source.name, trader_name, brought, target.name,
            ),
        );
        Ok(Outcome::Fired)
    })
}

/// Choose a living, unassigned survivor to travel.
/// Stays here because it applies Survivalist's traveling vendors rules through the game's classes, fields, content, and actions.
fn pick_free_member(com: &MonoObject) -> Result<Option<(i32, String)>, String> {
    let leader_id = handle_of(&com.read_field("Leader")?).map(|h| {
        own(h)
            .read_field("Id")
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(-1)
    });
    let Some(m_h) = handle_of(&com.read_field("Members")?) else {
        return Ok(None);
    };
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
            handle_of(&member.invoke("GetSquad", &json!([])).unwrap_or(Json::Null)).is_some();
        let id = member
            .read_field("Id")
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(-1);
        if !alive || !human || !conscious || squadded || Some(id) == leader_id {
            continue;
        }
        let name = member
            .invoke("GetDisplayNameString", &json!([]))
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "<unnamed>".into());
        std::mem::forget(member);
        return Ok(Some((h, name)));
    }
    Ok(None)
}

/// Check whether a camp already has a vendor on the road.
/// Stays here because it applies Survivalist's traveling vendors rules through the game's classes, fields, content, and actions.
fn mission_active_source(id: i64) -> bool {
    MISSIONS.lock().iter().any(|m| m.source_id == id)
}

// ---- Mission trait ---------------------------------------------------------

impl mission::Mission for Mission {
    modforge::mission_accessors!();

    /// Check whether the mission agent can continue.
    /// Stays here because it applies Survivalist's traveling vendors rules through the game's classes, fields, content, and actions.
    fn is_agent_alive(&self) -> Result<bool, String> {
        is_npc_alive(self.trader_h)
    }

    /// Resolve what happens when the mission reaches its destination.
    /// Stays here because it applies Survivalist's traveling vendors rules through the game's classes, fields, content, and actions.
    fn on_going(&mut self, _now: f32) -> Result<Step, String> {
        let target_alive = with(self.target_h, |h| {
            h.invoke("HasAnyLivingNonZombieMembers", &json!([]))
        })
        .map(|v| v == json!(true))
        .unwrap_or(false);
        if !target_alive {
            send_squad_home(self.source_h, self.squad_id, self.home)?;
            return Ok(Step::Transition);
        }
        if dist_sq_to_building(self.trader_h, self.target_h)? > ARRIVE_DIST_SQ {
            return Ok(Step::Continue);
        }
        self.paid = with(self.target_h, |h| {
            carry_off_stored_goods(
                h,
                &[self.trader_h],
                VENDOR_PAY_STACKS,
                GoodsFilter::NonFood,
                false,
            )
        })?;
        self.sold = deposit_goods(self.trader_h, self.target_h, self.brought)?;
        if self.target_is_player && self.sold > 0 {
            crate::chronicle::post(&format!(
                "a trader from {} has come to your gate with goods",
                self.source_name
            ));
        }
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: vendor: {} sells {} ware(s) to {} for {} in payment",
                self.trader_name, self.sold, self.target_name, self.paid,
            ),
        );
        send_squad_home(self.source_h, self.squad_id, self.home)?;
        Ok(Step::Transition)
    }

    /// Resolve what happens when the mission agent returns home.
    /// Stays here because it applies Survivalist's traveling vendors rules through the game's classes, fields, content, and actions.
    fn on_returning(&mut self, _now: f32) -> Result<Step, String> {
        if dist_sq_to_building(self.trader_h, self.source_h)? > ARRIVE_DIST_SQ {
            return Ok(Step::Continue);
        }
        if self.sold > 0 {
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: vendor: {} returns to {}; the trip to {} paid",
                    self.trader_name, self.source_name, self.target_name,
                ),
            );
        }
        Ok(Step::Complete)
    }

    /// Release the mission squad and managed handles when the mission ends.
    /// Stays here because it applies Survivalist's traveling vendors rules through the game's classes, fields, content, and actions.
    fn cleanup(self) {
        remove_squad_and_drop(
            self.source_h,
            self.squad_id,
            &[self.source_h, self.target_h, self.trader_h],
        );
    }

    /// Describe the active work for status output.
    /// Stays here because it applies Survivalist's traveling vendors rules through the game's classes, fields, content, and actions.
    fn label(&self) -> String {
        format!("{} to {}", self.source_name, self.target_name)
    }
}

/// Move up to `max` non-food stacks from the trader's carried
/// inventory into the target's first building that holds a
/// container: the delivery half of the swap, on the game's own
/// Take/Add calls.
/// Stays here because it applies Survivalist's traveling vendors rules through the game's classes, fields, content, and actions.
fn deposit_goods(trader_h: i32, target_h: i32, max: i64) -> Result<i64, String> {
    let store: Option<(i32, i32)> = with(target_h, |t| {
        let b_h = handle_of(&t.read_field("Buildings").ok()?)?;
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
    let mut moved = 0i64;
    while moved < max {
        let count = trader_inv.list_len().unwrap_or(0);
        let mut pick: Option<(i32, i64)> = None;
        for i in 0..count {
            let Some(item_h) = handle_of(&trader_inv.invoke("GetItem", &json!([i]))?) else {
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
        moved += 1;
    }
    drop(trader_inv);
    drop(store_inv);
    drop(own(store_bh));
    Ok(moved)
}
