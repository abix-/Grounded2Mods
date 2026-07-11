//! Bounty: the first slice of the work pillar (docs/status.md
//! "More to do (ecosystem-generated work)").
//!
//! A camp AT WAR posts a public bounty on its enemy's LEADER: a
//! chronicle line, no acceptance step. If the player's people
//! make the kill while the offer stands, the hirer loads real
//! payment from its stores onto a courier who walks it to the
//! player's gate. War over, mark dead by other hands, hirer
//! dead, or the window closing all void the offer.
//!
//! Kill attribution rides war.rs's OnMemberDied prefix (it calls
//! bounty::on_death); no hooks of its own.

use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::mono::{self, LogLevel, MonoObject};

use crate::common::{
    GoodsFilter, base_centre, carry_off_stored_goods, ctype, display_name, for_each_community,
    handle_of, on_main_thread, own, with,
};

/// Seconds between offer scans; work is a slow drumbeat, offset
/// from murder (240) and trade (150) so the acts interleave.
const BOUNTY_SCAN_PERIOD_SECS: f32 = 300.0;

/// Seconds between advance passes.
const MISSION_TICK_SECS: f32 = 5.0;

/// Real seconds an offer stands before it lapses.
const OFFER_WINDOW_SECS: f32 = 2700.0;

/// Non-food stacks the payment courier carries.
const BOUNTY_PAY_STACKS: i64 = 3;

/// A courier that has not resolved by then is recalled.
const COURIER_TIMEOUT_SECS: f32 = 1800.0;

/// Within this squared tile distance of a building the courier
/// has arrived; same bar trade uses.
const ARRIVE_DIST_SQ: f64 = 25.0;

enum Stage {
    Going,
    Returning,
}

/// The one bounty in flight map-wide. Each variant owns the
/// handles it names; transitions drop what they shed.
enum Bounty {
    Offered {
        hirer_h: i32,
        hirer_name: String,
        mark_h: i32,
        mark_id: i64,
        mark_name: String,
        enemy_name: String,
        /// Non-food stacks the hirer could pay at offer time; what
        /// the board and the chronicle advertise.
        pays: i64,
        /// The journal entry (a QuestInstance handle): the work
        /// board line the player reads, None when the quest data
        /// is not loaded yet (XML loads at story load).
        quest_h: Option<i32>,
        expires: f32,
    },
    /// The kill is confirmed; the next advance pass launches the
    /// payment courier (never inside the death callback).
    /// waiting_logged: the no-free-courier wait logs once, not
    /// every 5s pass.
    Owed {
        hirer_h: i32,
        hirer_name: String,
        mark_name: String,
        waiting_logged: bool,
    },
    /// The courier is on the road with real goods.
    Paying {
        hirer_h: i32,
        hirer_name: String,
        courier_h: i32,
        courier_name: String,
        player_h: i32,
        squad_id: i64,
        home: (i64, i64),
        stage: Stage,
        loaded: i64,
        deadline: f32,
    },
}

static BOUNTY: Mutex<Option<Bounty>> = Mutex::new(None);
static LAST_SCAN_BITS: AtomicU32 = AtomicU32::new(0);
static LAST_TICK_BITS: AtomicU32 = AtomicU32::new(0);
/// Game clock as of the last tick, for ops that need "now".
static LAST_NOW_BITS: AtomicU32 = AtomicU32::new(0);

pub fn tick(now: f32) {
    LAST_NOW_BITS.store(now.to_bits(), Ordering::Relaxed);
    let last_tick = f32::from_bits(LAST_TICK_BITS.load(Ordering::Relaxed));
    if now - last_tick >= MISSION_TICK_SECS {
        LAST_TICK_BITS.store(now.to_bits(), Ordering::Relaxed);
        advance(now);
    }
    let last_scan = f32::from_bits(LAST_SCAN_BITS.load(Ordering::Relaxed));
    if now - last_scan >= BOUNTY_SCAN_PERIOD_SECS {
        LAST_SCAN_BITS.store(now.to_bits(), Ordering::Relaxed);
        if BOUNTY.lock().is_some() {
            return;
        }
        if let Err(e) = offer_scan(now) {
            if !e.contains("not found") {
                mono::log(LogLevel::Warn, &format!("survivalist-mod: bounty scan failed: {e}"));
            }
        }
    }
}

// ---- the offer ---------------------------------------------------------------

fn offer_scan(now: f32) -> Result<(), String> {
    // Board entries from a prior generation or a loaded save have
    // no owner in this process; clear them before posting fresh.
    board_sweep_orphans();

    // The player's camp: bounties exist to be claimed by them.
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

    // The hirer: at war with another AI camp, friendly enough to
    // the player, able to pay; the smallest camp first (the side
    // losing its war hires the help).
    let mut pick: Option<(i32, String, i64, i32)> = None; // hirer_h, name, members, enemy_h
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
        let Some(enemy_h) = handle_of(&com.read_field("InvasionTarget")?) else {
            return Ok(true);
        };
        let enemy_ok = with(enemy_h, |e| {
            let et = ctype(e);
            (et == "Normal" || et == "Looter")
                && e.invoke("IsAISettlement", &json!([]))
                    .map(|v| v == json!(true))
                    .unwrap_or(false)
        });
        let friendly = com
            .invoke("GetRelationship", &json!([{ "handle": player_h }]))
            .map(|r| r != json!("Hostile"))
            .unwrap_or(false);
        if !enemy_ok || !friendly || count_stored_goods(&com, GoodsFilter::NonFood, 1) == 0 {
            drop(own(enemy_h));
            return Ok(true);
        }
        if pick.as_ref().map(|p| members < p.2).unwrap_or(true) {
            if let Some((old_h, _, _, old_e)) =
                pick.replace((com.handle().0, display_name(&com), members, enemy_h))
            {
                drop(own(old_h));
                drop(own(old_e));
            }
            std::mem::forget(com);
        } else {
            drop(own(enemy_h));
        }
        Ok(true)
    })?;
    drop(own(player_h));
    let Some((hirer_h, hirer_name, _, enemy_h)) = pick else {
        return Ok(());
    };
    post_offer(hirer_h, hirer_name, enemy_h, now)
}

/// Turn a hirer + enemy pair into a standing offer on the enemy's
/// leader. Consumes both handles (keeps hirer + mark, drops the
/// enemy community).
fn post_offer(hirer_h: i32, hirer_name: String, enemy_h: i32, now: f32) -> Result<(), String> {
    let enemy = own(enemy_h);
    let enemy_name = display_name(&enemy);
    let mark = handle_of(&enemy.read_field("Leader")?).and_then(|h| {
        let alive = with(h, |v| {
            v.invoke("get_AliveAndNotZombie", &json!([]))
                .map(|x| x == json!(true))
                .unwrap_or(false)
        });
        if alive {
            Some(h)
        } else {
            drop(own(h));
            None
        }
    });
    drop(enemy);
    let Some(mark_h) = mark else {
        drop(own(hirer_h));
        return Ok(());
    };
    let (mark_id, mark_name) = with(mark_h, |v| {
        (
            v.read_field("Id").ok().and_then(|x| x.as_i64()).unwrap_or(-1),
            v.invoke("GetDisplayNameString", &json!([]))
                .ok()
                .and_then(|x| x.as_str().map(str::to_string))
                .unwrap_or_else(|| "<their leader>".into()),
        )
    });

    // What the offer pays, counted from the real stores NOW so
    // the board can advertise it. A broke hirer posts nothing.
    let pays = with(hirer_h, |com| {
        count_stored_goods(com, GoodsFilter::NonFood, BOUNTY_PAY_STACKS)
    });
    if pays == 0 {
        drop(own(hirer_h));
        drop(own(mark_h));
        return Ok(());
    }

    // The work board: the game's own journal entry with a map
    // marker tracking the mark. Best-effort: the offer stands
    // (chronicle + status) even when the quest data is missing.
    let quest_h = board_spawn(hirer_h, mark_h);

    crate::chronicle::post(&format!(
        "{hirer_name} offers a bounty on {mark_name}, leader of {enemy_name}: pays {pays} stack(s) of goods"
    ));
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: bounty: {hirer_name} posts a bounty on {mark_name}, leader of {enemy_name}, paying {pays} stack(s) (window {OFFER_WINDOW_SECS}s)"
        ),
    );
    *BOUNTY.lock() = Some(Bounty::Offered {
        hirer_h,
        hirer_name,
        mark_h,
        mark_id,
        mark_name,
        enemy_name,
        pays,
        quest_h,
        expires: now + OFFER_WINDOW_SECS,
    });
    Ok(())
}

// ---- the work board (the game's own quest journal) -----------------------------

/// The quest data shipped in story/Scripts/WorkBoard.xml.
const BOARD_QUEST_ID: &str = "WorkBoard_Bounty";

/// Spawn the journal entry for an offer via the game's own
/// QuestInstance.Spawn: new-quest notification, journal line
/// ("%1 offers a bounty on %2, leader of %3..."), and a map
/// marker tracking the mark. Returns the instance handle; None
/// (with a log line) when the quest data is not loaded, since the
/// XML loads at story load and a hot reload alone cannot see it.
fn board_spawn(hirer_h: i32, mark_h: i32) -> Option<i32> {
    let quest_h = match find_board_quest() {
        Ok(Some(h)) => h,
        Ok(None) => {
            mono::log(
                LogLevel::Info,
                "survivalist-mod: bounty: WorkBoard_Bounty quest data not loaded; no journal entry (restart the story to load Scripts/WorkBoard.xml)",
            );
            return None;
        }
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: bounty: board quest lookup failed: {e}"),
            );
            return None;
        }
    };
    // Giver: the hirer's leader; seeker: the player's leader. The
    // description's community names resolve from these characters.
    let giver_h = with(hirer_h, |c| c.read_field("Leader").ok().as_ref().and_then(handle_of));
    let mut seeker_h: Option<i32> = None;
    let _ = for_each_community(|com| {
        if ctype(&com) == "Player" {
            seeker_h = com.read_field("Leader").ok().as_ref().and_then(handle_of);
            return Ok(false);
        }
        Ok(true)
    });
    let giver = giver_h.map(|h| json!({"handle": h})).unwrap_or(Json::Null);
    let seeker = seeker_h.map(|h| json!({"handle": h})).unwrap_or(Json::Null);
    let spawned = mono::invoke_static(
        "QuestInstance",
        "Spawn",
        &json!([{ "handle": quest_h }, giver, seeker, { "handle": mark_h }, false]),
    );
    drop(own(quest_h));
    if let Some(h) = giver_h {
        drop(own(h));
    }
    if let Some(h) = seeker_h {
        drop(own(h));
    }
    match spawned {
        Ok(v) => handle_of(&v),
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: bounty: board entry spawn failed: {e}"),
            );
            None
        }
    }
}

/// Walk GameImpl.Instance.CurrentStories and ask each loaded
/// story for the quest data (Story.FindQuestByUniqueID); the one
/// that loaded our XML answers.
fn find_board_quest() -> Result<Option<i32>, String> {
    let game = mono::MonoType::find("GameImpl")
        .and_then(|t| t.singleton_instance())
        .ok_or("GameImpl.Instance not found")?;
    let Some(list_h) = handle_of(&game.read_field("CurrentStories")?) else {
        return Ok(None);
    };
    let list = own(list_h);
    let n = list.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
    for i in 0..n {
        let Some(story_h) = handle_of(&list.invoke("get_Item", &json!([i]))?) else {
            continue;
        };
        let story = own(story_h);
        if let Ok(q) = story.invoke("FindQuestByUniqueID", &json!([BOARD_QUEST_ID])) {
            if let Some(qh) = handle_of(&q) {
                return Ok(Some(qh));
            }
        }
    }
    Ok(None)
}

/// Resolve the journal entry: Complete (claimed) or Fail (lapsed
/// or void), both the game's own paths with their own
/// notifications. Consumes the handle.
fn board_close(quest_h: Option<i32>, claimed: bool) {
    let Some(h) = quest_h else { return };
    // The 1-arg overloads (skipCompletionEvents: false) avoid any
    // 0-arg/1-arg overload ambiguity in the shim's resolution.
    let method = if claimed { "Complete" } else { "Fail" };
    if let Err(e) = with(h, |q| q.invoke(method, &json!([false]))) {
        mono::log(
            LogLevel::Warn,
            &format!("survivalist-mod: bounty: board {method} failed: {e}"),
        );
    }
    drop(own(h));
}

/// Delete every active board entry: a prior generation's (hot
/// reload) or a loaded save's entries have no owner in this
/// process and would linger in the journal forever.
fn board_sweep_orphans() {
    let Some(sm) = mono::MonoType::find("StoryManager").and_then(|t| t.singleton_instance())
    else {
        return;
    };
    let Some(list_h) = sm.read_field("ActiveQuests").ok().as_ref().and_then(handle_of) else {
        return;
    };
    let list = own(list_h);
    let n = list
        .invoke("get_Count", &json!([]))
        .ok()
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    // Collect first: Delete mutates the list.
    let mut orphans = Vec::new();
    for i in 0..n {
        let Some(h) = list
            .invoke("get_Item", &json!([i]))
            .ok()
            .as_ref()
            .and_then(handle_of)
        else {
            continue;
        };
        let q = own(h);
        let is_ours = q
            .invoke("GetUniqueID", &json!([]))
            .ok()
            .and_then(|v| v.as_str().map(|s| s.starts_with(BOARD_QUEST_ID)))
            .unwrap_or(false);
        if is_ours {
            std::mem::forget(q);
            orphans.push(h);
        }
    }
    let count = orphans.len();
    for h in orphans {
        let _ = with(h, |q| q.invoke("Delete", &json!([])));
        drop(own(h));
    }
    if count > 0 {
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: bounty: swept {count} orphaned board entries from a prior generation or save"
            ),
        );
    }
}

/// Count the community's stored stacks matching the filter, up to
/// `cap` (early exit; cap 1 is a cheap "has any" test, cap
/// BOUNTY_PAY_STACKS is the advertised reward).
fn count_stored_goods(com: &MonoObject, filter: GoodsFilter, cap: i64) -> i64 {
    let Some(b_h) = com.read_field("Buildings").ok().as_ref().and_then(handle_of) else {
        return 0;
    };
    let mut found = 0i64;
    let blist = own(b_h);
    let nb = blist
        .invoke("get_Count", &json!([]))
        .ok()
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    for bi in 0..nb {
        let Some(bh) = blist
            .invoke("get_Item", &json!([bi]))
            .ok()
            .as_ref()
            .and_then(handle_of)
        else {
            continue;
        };
        let building = own(bh);
        let Some(inv_h) = building.read_field("Inventory").ok().as_ref().and_then(handle_of)
        else {
            continue;
        };
        let inv = own(inv_h);
        let n = inv
            .invoke("get_Count", &json!([]))
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        for i in 0..n {
            let Some(item_h) = inv
                .invoke("GetItem", &json!([i]))
                .ok()
                .as_ref()
                .and_then(handle_of)
            else {
                continue;
            };
            let item = own(item_h);
            if matches_filter(&item, filter) {
                found += 1;
                if found >= cap {
                    return found;
                }
            }
        }
    }
    found
}

/// GoodsFilter::matches is private to common.rs; the same food
/// test, restated (GetNutrition > 0 is food, per common.rs:184).
fn matches_filter(item: &MonoObject, filter: GoodsFilter) -> bool {
    let n = item
        .invoke("GetNutrition", &json!([]))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    match filter {
        GoodsFilter::Any => true,
        GoodsFilter::Food => n > 0.0,
        GoodsFilter::NonFood => n <= 0.0,
    }
}

// ---- kill attribution ---------------------------------------------------------

/// Called from war.rs's OnMemberDied prefix for every death.
/// Cheap gate first: no open offer means no bridge calls. Never
/// launches anything here; the courier launch belongs to the
/// tick, outside the game's death processing.
pub fn on_death(member: &MonoObject) {
    {
        let slot = BOUNTY.lock();
        if !matches!(slot.as_ref(), Some(Bounty::Offered { .. })) {
            return;
        }
    }
    let Some(dead_id) = member.read_field("Id").ok().and_then(|v| v.as_i64()) else {
        return;
    };
    let mut slot = BOUNTY.lock();
    let Some(Bounty::Offered { mark_id, .. }) = slot.as_ref() else {
        return;
    };
    if *mark_id != dead_id {
        return;
    }
    // The mark is down. By whose hand?
    let by_player = (|| {
        let kh = handle_of(&member.read_field("Killer").ok()?)?;
        let killer = own(kh);
        let ch = handle_of(&killer.read_field("Community").ok()?)?;
        Some(ctype(&own(ch)) == "Player")
    })()
    .unwrap_or(false);
    let Some(Bounty::Offered { hirer_h, hirer_name, mark_h, mark_name, enemy_name, quest_h, .. }) =
        slot.take()
    else {
        return;
    };
    drop(own(mark_h));
    if by_player {
        board_close(quest_h, true);
        crate::chronicle::post(&format!(
            "the bounty on {mark_name} is claimed; {hirer_name} owes a debt"
        ));
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: bounty: {mark_name} of {enemy_name} fell to the player; {hirer_name} owes payment"
            ),
        );
        *slot = Some(Bounty::Owed { hirer_h, hirer_name, mark_name, waiting_logged: false });
    } else {
        board_close(quest_h, false);
        mono::log(
            LogLevel::Info,
            &format!("survivalist-mod: bounty: {mark_name} died by other hands; the offer lapses"),
        );
        drop(own(hirer_h));
    }
}

// ---- advancing ---------------------------------------------------------------

fn advance(now: f32) {
    let mut slot = BOUNTY.lock();
    let Some(state) = slot.take() else { return };
    *slot = match state {
        Bounty::Offered {
            hirer_h,
            hirer_name,
            mark_h,
            mark_id,
            mark_name,
            enemy_name,
            pays,
            quest_h,
            expires,
        } => {
            if advance_offered(hirer_h, &hirer_name, mark_h, &mark_name, expires, now) {
                board_close(quest_h, false);
                drop(own(hirer_h));
                drop(own(mark_h));
                None
            } else {
                Some(Bounty::Offered {
                    hirer_h,
                    hirer_name,
                    mark_h,
                    mark_id,
                    mark_name,
                    enemy_name,
                    pays,
                    quest_h,
                    expires,
                })
            }
        }
        Bounty::Owed { hirer_h, hirer_name, mark_name, waiting_logged } => {
            launch_courier(hirer_h, hirer_name, mark_name, waiting_logged, now)
        }
        s @ Bounty::Paying { .. } => advance_paying(s, now),
    };
}

/// The debt comes due: load real payment onto the hirer's first
/// free member and walk it to the player's gate. Returns the next
/// state (stays Owed while no member is free).
fn launch_courier(
    hirer_h: i32,
    hirer_name: String,
    mark_name: String,
    waiting_logged: bool,
    now: f32,
) -> Option<Bounty> {
    // The player's gate.
    let mut player: Option<(i32, (i64, i64))> = None;
    let _ = for_each_community(|com| {
        if ctype(&com) == "Player" {
            if let Some(c) = base_centre(&com) {
                player = Some((com.handle().0, c));
                std::mem::forget(com);
            }
            return Ok(false);
        }
        Ok(true)
    });
    let Some((player_h, dest)) = player else {
        mono::log(
            LogLevel::Info,
            &format!("survivalist-mod: bounty: no player camp to pay; {hirer_name}'s debt is void"),
        );
        drop(own(hirer_h));
        return None;
    };
    // A hirer that died owing pays nothing.
    let standing = with(hirer_h, |c| {
        c.invoke("HasAnyLivingNonZombieMembers", &json!([]))
            .map(|v| v == json!(true))
            .unwrap_or(false)
    });
    if !standing {
        mono::log(
            LogLevel::Info,
            &format!("survivalist-mod: bounty: {hirer_name} died owing; the debt dies with them"),
        );
        drop(own(hirer_h));
        drop(own(player_h));
        return None;
    }
    // The courier: the first free member.
    let courier = match with(hirer_h, |com| pick_courier(com)) {
        Ok(c) => c,
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: bounty: courier pick failed: {e}"),
            );
            None
        }
    };
    let Some((courier_h, courier_name)) = courier else {
        if !waiting_logged {
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: bounty: {hirer_name} owes for {mark_name} but has no free member to send; waiting"
                ),
            );
        }
        drop(own(player_h));
        return Some(Bounty::Owed { hirer_h, hirer_name, mark_name, waiting_logged: true });
    };
    // Load the payment from real stores.
    let loaded = with(hirer_h, |com| {
        carry_off_stored_goods(com, &[courier_h], BOUNTY_PAY_STACKS, GoodsFilter::NonFood)
    })
    .unwrap_or(0);
    if loaded == 0 {
        crate::chronicle::post(&format!("{hirer_name} cannot pay the bounty"));
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: bounty: {hirer_name}'s stores are bare; the debt for {mark_name} goes unpaid"
            ),
        );
        drop(own(hirer_h));
        drop(own(courier_h));
        drop(own(player_h));
        return None;
    }
    // On the road as a real 1-member Trade squad.
    let home = with(hirer_h, |com| base_centre(com)).unwrap_or(dest);
    let dest_j = json!({"x": dest.0, "y": dest.1});
    let squad_id = match with(hirer_h, |com| -> Result<i64, String> {
        let squad_h = handle_of(&com.invoke("AddSquad", &json!(["Trade", 0]))?)
            .ok_or("AddSquad gave no squad")?;
        let squad = own(squad_h);
        com.invoke(
            "AddToSquad",
            &json!([{ "handle": courier_h }, { "handle": squad_h }]),
        )?;
        squad.write_field("GoalTile", &dest_j)?;
        com.invoke(
            "SetSquadAction",
            &json!([{ "handle": squad_h }, "GoTo", 0, dest_j.clone(), null, false]),
        )?;
        squad.read_field("Id").map(|v| v.as_i64().unwrap_or(-1))
    }) {
        Ok(id) => id,
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: bounty: courier launch failed: {e}"),
            );
            drop(own(hirer_h));
            drop(own(courier_h));
            drop(own(player_h));
            return None;
        }
    };
    crate::chronicle::post(&format!("{hirer_name} sends payment for the bounty on {mark_name}"));
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: bounty: {hirer_name} sends {courier_name} with {loaded} stack(s) of payment to the player's gate"
        ),
    );
    Some(Bounty::Paying {
        hirer_h,
        hirer_name,
        courier_h,
        courier_name,
        player_h,
        squad_id,
        home,
        stage: Stage::Going,
        loaded,
        deadline: now + COURIER_TIMEOUT_SECS,
    })
}

/// One courier step. Returns the next state (None = closed).
fn advance_paying(state: Bounty, now: f32) -> Option<Bounty> {
    let Bounty::Paying {
        hirer_h,
        hirer_name,
        courier_h,
        courier_name,
        player_h,
        squad_id,
        home,
        stage,
        loaded,
        deadline,
    } = state
    else {
        return Some(state);
    };
    let alive = with(courier_h, |c| c.invoke("get_AliveAndNotZombie", &json!([])))
        .map(|v| v == json!(true))
        .unwrap_or(false);
    if !alive {
        crate::chronicle::post(&format!("the bounty payment from {hirer_name} never arrived"));
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: bounty: courier {courier_name} died on the road; the payment is lost"
            ),
        );
        close_paying(hirer_h, courier_h, player_h, squad_id);
        return None;
    }
    if now >= deadline {
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: bounty: courier {courier_name} recalled (timeout); the payment never arrived"
            ),
        );
        close_paying(hirer_h, courier_h, player_h, squad_id);
        return None;
    }
    let tile = match with(courier_h, |c| c.invoke("get_Tile", &json!([]))) {
        Ok(t) => t,
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: bounty: courier tile read failed: {e}"),
            );
            close_paying(hirer_h, courier_h, player_h, squad_id);
            return None;
        }
    };
    match stage {
        Stage::Going => {
            let d = with(player_h, |p| {
                p.invoke("GetDistSqToNearestBuilding", &json!([tile.clone()]))
            })
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(f64::MAX);
            if d > ARRIVE_DIST_SQ {
                return Some(Bounty::Paying {
                    hirer_h,
                    hirer_name,
                    courier_h,
                    courier_name,
                    player_h,
                    squad_id,
                    home,
                    stage: Stage::Going,
                    loaded,
                    deadline,
                });
            }
            // At the gate: real hands into the player's store.
            let delivered = deliver_carried_payment(courier_h, player_h, loaded).unwrap_or(0);
            if delivered > 0 {
                crate::chronicle::post(&format!(
                    "a courier from {hirer_name} brings your bounty payment"
                ));
            }
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: bounty: {courier_name} delivers {delivered} stack(s) into the player's store"
                ),
            );
            // Walk home.
            let home_j = json!({"x": home.0, "y": home.1});
            let _ = with(hirer_h, |com| -> Result<(), String> {
                if let Ok(sq) = com.invoke("GetSquad", &json!([squad_id])) {
                    if let Some(sq_h) = handle_of(&sq) {
                        let squad = own(sq_h);
                        squad.write_field("GoalTile", &home_j)?;
                        com.invoke(
                            "SetSquadAction",
                            &json!([{ "handle": sq_h }, "GoTo", 0, home_j.clone(), null, false]),
                        )?;
                    }
                }
                Ok(())
            });
            Some(Bounty::Paying {
                hirer_h,
                hirer_name,
                courier_h,
                courier_name,
                player_h,
                squad_id,
                home,
                stage: Stage::Returning,
                loaded: delivered,
                deadline,
            })
        }
        Stage::Returning => {
            let d = with(hirer_h, |com| {
                com.invoke("GetDistSqToNearestBuilding", &json!([tile]))
            })
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(f64::MAX);
            if d <= ARRIVE_DIST_SQ {
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: bounty: {courier_name} home; the bounty is paid and closed"
                    ),
                );
                close_paying(hirer_h, courier_h, player_h, squad_id);
                return None;
            }
            Some(Bounty::Paying {
                hirer_h,
                hirer_name,
                courier_h,
                courier_name,
                player_h,
                squad_id,
                home,
                stage: Stage::Returning,
                loaded,
                deadline,
            })
        }
    }
}

/// Disband the courier squad and release every held handle.
fn close_paying(hirer_h: i32, courier_h: i32, player_h: i32, squad_id: i64) {
    with(hirer_h, |com| {
        if let Ok(sq) = com.invoke("GetSquad", &json!([squad_id])) {
            if let Some(sq_h) = handle_of(&sq) {
                let _ = com.invoke("RemoveSquad", &json!([{ "handle": sq_h }]));
            }
        }
    });
    drop(own(hirer_h));
    drop(own(courier_h));
    drop(own(player_h));
}

/// The first free member: alive, human, conscious, unsquadded,
/// not the leader (murder.rs's eligibility, no genome ranking).
fn pick_courier(com: &MonoObject) -> Result<Option<(i32, String)>, String> {
    let leader_id = handle_of(&com.read_field("Leader")?)
        .map(|h| own(h).read_field("Id").ok().and_then(|v| v.as_i64()).unwrap_or(-1));
    let Some(m_h) = handle_of(&com.read_field("Members")?) else {
        return Ok(None);
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

/// Move up to `max` non-food stacks from the courier's carried
/// inventory into the player's first storage building: the payout,
/// on the same Take/Add calls as everything else (trade.rs's
/// delivery, filter inverted).
fn deliver_carried_payment(courier_h: i32, player_h: i32, max: i64) -> Result<i64, String> {
    // The receiving shelf: the player's first building with an
    // inventory container.
    let store: Option<(i32, i32)> = with(player_h, |host| {
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
    let courier_inv_h = with(courier_h, |c| {
        handle_of(&c.read_field("Inventory")?).ok_or("courier has no inventory".to_string())
    })?;
    let courier_inv = own(courier_inv_h);
    let store_inv = own(store_inv_h);
    let mut delivered = 0i64;
    while delivered < max {
        let count = courier_inv
            .invoke("get_Count", &json!([]))
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let mut pick: Option<(i32, i64)> = None;
        for i in 0..count {
            let Some(item_h) = handle_of(&courier_inv.invoke("GetItem", &json!([i]))?) else {
                continue;
            };
            let item = own(item_h);
            let amount = item
                .invoke("GetAmount", &json!([]))
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(1);
            if matches_filter(&item, GoodsFilter::NonFood) {
                std::mem::forget(item);
                pick = Some((item_h, amount));
                break;
            }
        }
        let Some((item_h, amount)) = pick else { break };
        let taken = courier_inv.invoke(
            "Take",
            &json!([{ "handle": courier_h }, { "handle": item_h }, amount]),
        )?;
        let Some(taken_h) = handle_of(&taken) else { break };
        store_inv.invoke(
            "Add",
            &json!([{ "handle": store_bh }, { "handle": taken_h }]),
        )?;
        delivered += 1;
    }
    drop(courier_inv);
    drop(store_inv);
    drop(own(store_bh));
    Ok(delivered)
}

/// True = the offer is void; clean up.
fn advance_offered(
    hirer_h: i32,
    hirer_name: &str,
    mark_h: i32,
    mark_name: &str,
    expires: f32,
    now: f32,
) -> bool {
    if now >= expires {
        mono::log(
            LogLevel::Info,
            &format!("survivalist-mod: bounty: the offer on {mark_name} lapses unclaimed"),
        );
        return true;
    }
    // The hirer must still stand and still be at war.
    let hirer_standing = with(hirer_h, |c| {
        c.invoke("HasAnyLivingNonZombieMembers", &json!([]))
            .map(|v| v == json!(true))
            .unwrap_or(false)
    });
    let at_war = with(hirer_h, |c| {
        c.read_field("InvasionTarget")
            .ok()
            .as_ref()
            .and_then(handle_of)
            .map(|h| {
                drop(own(h));
                true
            })
            .unwrap_or(false)
    });
    if !hirer_standing || !at_war {
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: bounty: {hirer_name}'s war is over; the offer on {mark_name} is void"
            ),
        );
        return true;
    }
    // Belt over the hook's braces: a mark found dead here (a death
    // path with no Killer, or attribution not yet wired) voids the
    // offer.
    let mark_alive = with(mark_h, |v| {
        v.invoke("get_AliveAndNotZombie", &json!([]))
            .map(|x| x == json!(true))
            .unwrap_or(false)
    });
    if !mark_alive {
        mono::log(
            LogLevel::Info,
            &format!("survivalist-mod: bounty: {mark_name} died by other hands; the offer lapses"),
        );
        return true;
    }
    false
}

// ---- ops ---------------------------------------------------------------------

pub fn register_ops() {
    OP_REGISTRY.register_many([
        OpDef::new(
            "bounty_status",
            "The one open bounty (offered/owed/paying) or null. The work-pillar observability surface.",
            "{}",
            bounty_status,
        ),
        OpDef::new(
            "bounty_post",
            "Force an offer from a named camp that is at war (reads its InvasionTarget's leader). Live-verification probe, like war_ignite.",
            "{hirer: str}",
            bounty_post,
        ),
    ]);
}

fn bounty_status(_args: &Json) -> Result<Json, String> {
    let now = f32::from_bits(LAST_NOW_BITS.load(Ordering::Relaxed));
    let slot = BOUNTY.lock();
    Ok(match slot.as_ref() {
        None => json!({ "bounty": null }),
        Some(Bounty::Offered { hirer_name, mark_name, enemy_name, pays, quest_h, expires, .. }) => {
            json!({
                "bounty": {
                    "stage": "offered",
                    "hirer": hirer_name,
                    "mark": mark_name,
                    "of": enemy_name,
                    "pays": pays,
                    "board": quest_h.is_some(),
                    "expires_in_secs": (expires - now).max(0.0),
                }
            })
        }
        Some(Bounty::Owed { hirer_name, mark_name, .. }) => json!({
            "bounty": { "stage": "owed", "hirer": hirer_name, "mark": mark_name }
        }),
        Some(Bounty::Paying { hirer_name, courier_name, loaded, stage, .. }) => json!({
            "bounty": {
                "stage": "paying",
                "hirer": hirer_name,
                "courier": courier_name,
                "stacks": loaded,
                "leg": match stage {
                    Stage::Going => "going",
                    Stage::Returning => "returning",
                },
            }
        }),
    })
}

fn bounty_post(args: &Json) -> Result<Json, String> {
    let hirer = args
        .get("hirer")
        .and_then(Json::as_str)
        .ok_or("missing arg 'hirer' (community display name)")?
        .to_string();
    on_main_thread(move || {
        if BOUNTY.lock().is_some() {
            return Err("a bounty is already open (bounty_status)".into());
        }
        board_sweep_orphans();
        let now = f32::from_bits(LAST_NOW_BITS.load(Ordering::Relaxed));
        let mut found: Option<(i32, String, i32)> = None;
        for_each_community(|com| {
            if display_name(&com).eq_ignore_ascii_case(&hirer) {
                let enemy_h = handle_of(&com.read_field("InvasionTarget")?);
                let name = display_name(&com);
                let h = com.handle().0;
                match enemy_h {
                    Some(e) => {
                        found = Some((h, name, e));
                        std::mem::forget(com);
                    }
                    None => return Err(format!("'{name}' is not at war (no InvasionTarget)")),
                }
                return Ok(false);
            }
            Ok(true)
        })?;
        let Some((hirer_h, hirer_name, enemy_h)) = found else {
            return Err(format!("hirer community '{hirer}' not found"));
        };
        post_offer(hirer_h, hirer_name.clone(), enemy_h, now)?;
        match &*BOUNTY.lock() {
            Some(Bounty::Offered { mark_name, enemy_name, pays, .. }) => Ok(json!({
                "posted": true, "hirer": hirer_name, "mark": mark_name, "of": enemy_name,
                "pays": pays,
            })),
            _ => Err(format!(
                "'{hirer_name}' posted nothing (no living enemy leader, or empty stores)"
            )),
        }
    })
}
