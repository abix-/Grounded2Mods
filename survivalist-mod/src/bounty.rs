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
    GoodsFilter, ctype, display_name, for_each_community, handle_of, on_main_thread, own, with,
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
        expires: f32,
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
        expires: now + OFFER_WINDOW_SECS,
    });
    Ok(())
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

// ---- advancing ---------------------------------------------------------------

fn advance(now: f32) {
    let mut slot = BOUNTY.lock();
    let done = match slot.as_mut() {
        None => return,
        Some(Bounty::Offered { hirer_h, hirer_name, mark_h, mark_name, expires, .. }) => {
            advance_offered(*hirer_h, hirer_name, *mark_h, mark_name, *expires, now)
        }
    };
    if done {
        if let Some(Bounty::Offered { hirer_h, mark_h, .. }) = slot.take() {
            drop(own(hirer_h));
            drop(own(mark_h));
        }
    }
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
        Some(Bounty::Offered { hirer_name, mark_name, enemy_name, pays, expires, .. }) => json!({
            "bounty": {
                "stage": "offered",
                "hirer": hirer_name,
                "mark": mark_name,
                "of": enemy_name,
                "pays": pays,
                "expires_in_secs": (expires - now).max(0.0),
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
