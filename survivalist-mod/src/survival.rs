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

use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::mono::{self, LogLevel, MonoObject};

use crate::common::{
    ctype, display_name, for_each_community, handle_of, list_len, on_main_thread, own,
};

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
    OP_REGISTRY.register_many([OpDef::new(
        "survival_status",
        "Every AI settlement's nutrition, population vs worldgen size vs beds, threat count, and computed desperation rung (comfortable/strained/desperate/terminal). The living-struggle observability surface.",
        "{}",
        survival_status,
    )]);
}

// ---- the desperation tick ---------------------------------------------------

static LAST_SCAN_BITS: AtomicU32 = AtomicU32::new(0);

pub fn tick(now: f32) {
    let last = f32::from_bits(LAST_SCAN_BITS.load(Ordering::Relaxed));
    if now - last < SURVIVAL_SCAN_PERIOD_SECS {
        return;
    }
    LAST_SCAN_BITS.store(now.to_bits(), Ordering::Relaxed);
    if let Err(e) = desperation_scan() {
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
    name: String,
    ctype: String,
    nutrition: f64,
    members: i64,
    rung: Rung,
    centre: Option<(i64, i64)>,
    already_at_war: bool,
}

fn base_centre(com: &MonoObject) -> Option<(i64, i64)> {
    let rect = com.read_field("BaseRect").ok()?;
    let o = rect.as_object()?;
    let min = o.get("min")?.as_object()?;
    let max = o.get("max")?.as_object()?;
    let g = |m: &serde_json::Map<String, Json>, k: &str| m.get(k).and_then(Json::as_i64);
    Some((
        (g(min, "x")? + g(max, "x")?) / 2,
        (g(min, "y")? + g(max, "y")?) / 2,
    ))
}

fn desperation_scan() -> Result<(), String> {
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
        let already_at_war = handle_of(&com.read_field("InvasionTarget")?).is_some();
        camps.push(Camp {
            handle: com.handle().0,
            name: display_name(&com),
            ctype: t,
            nutrition: sv.nutrition,
            members: sv.members,
            rung: sv.rung,
            centre: base_centre(&com),
            already_at_war,
        });
        std::mem::forget(com); // handles reused across the two passes below
        Ok(true)
    })?;

    // Pick ONE desperate, hungry, not-already-warring raider and
    // send it at the nearest well-fed neighbor. One per scan keeps
    // the map's escalation organic.
    let raider = camps.iter().find(|c| {
        c.rung == Rung::Desperate
            && c.nutrition <= DESPERATE_NUTRITION
            && !c.already_at_war
            && c.members >= 2
            && c.centre.is_some()
    });
    let Some(raider) = raider else {
        release_camps(&camps);
        return Ok(());
    };
    let (rx, ry) = raider.centre.unwrap();

    // Best target: well-fed, not the raider, nearest by base
    // centre.
    let mut best: Option<(&Camp, i64)> = None;
    for c in &camps {
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
        release_camps(&camps);
        return Ok(());
    };

    // Drive it through the game's own machinery (same as war.rs's
    // war_ignite): hostile + invasion.
    let raider_obj = own(raider.handle);
    let target_h = target.handle;
    let cm = crate::common::community_manager()?;
    cm.invoke(
        "SetRelationship",
        &json!([{ "handle": raider.handle }, { "handle": target_h }, "Hostile"]),
    )?;
    raider_obj.invoke(
        "SetInvasionTarget",
        &json!([{ "handle": target_h }, 7.0, false]),
    )?;
    std::mem::forget(raider_obj);

    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: survival -- {} ({}, starving at {:.2}) raids {} for food (well-fed at {:.2})",
            raider.name,
            raider.ctype,
            raider.nutrition,
            target.name,
            target.nutrition
        ),
    );

    release_camps(&camps);
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
            let invasion_target = match handle_of(&com.read_field("InvasionTarget")?) {
                Some(h) => Json::String(display_name(&own(h))),
                None => Json::Null,
            };
            out.push(json!({
                "name": display_name(&com),
                "type": t,
                "rung": sv.rung.name(),
                "nutrition": (sv.nutrition * 100.0).round() / 100.0,
                "members": sv.members,
                "initial": sv.initial,
                "beds": sv.beds,
                "threats": sv.threats,
                "raiding": invasion_target,
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
