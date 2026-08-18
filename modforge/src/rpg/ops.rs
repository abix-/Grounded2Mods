//! Generic RPG op handlers for the framework's HTTP endpoint.
//!
//! Game crates register the standard op set against
//! [`crate::ops::OP_REGISTRY`] in one call:
//!
//! ```ignore
//! modforge::rpg::ops::register(&TRACKER);
//! ```
//!
//! Registers: `skill_toggle`, `skill_spend`, `skill_refund`,
//! `reload_slot`, `set_skill_points`, `skill_state`,
//! `skill_add_xp`. Generic over the engine type so both
//! frameworks reuse the same handlers.

use serde_json::Value as Json;

use crate::args::{arg_bool, arg_str, arg_u64};
use crate::ops::{OP_REGISTRY, OpDef};
use crate::rpg::engine::Engine;
use crate::rpg::tracker::Tracker;

/// Register the standard RPG op set against [`OP_REGISTRY`]
/// with the game's static `Tracker<E>` captured. Call once at
/// worker init.
pub fn register<E: Engine>(tracker: &'static Tracker<E>) {
    OP_REGISTRY.register_many([
        OpDef::new(
            "skill_toggle",
            "Enable / disable a skill without changing its level",
            "{id: str, enabled: bool}",
            move |args| skill_toggle(tracker, args),
        ),
        OpDef::new(
            "skill_spend",
            "Spend N skill points on a skill",
            "{id: str, count?: u32}",
            move |args| skill_spend(tracker, args),
        ),
        OpDef::new(
            "skill_refund",
            "Refund N points from a skill",
            "{id: str, count?: u32}",
            move |args| skill_refund(tracker, args),
        ),
        OpDef::new(
            "reload_slot",
            "Re-apply every catalog skill against current state",
            "{}",
            move |_args| reload_slot(tracker),
        ),
        OpDef::new(
            "set_skill_points",
            "Grant N unspent skill points (debug)",
            "{count: u64}",
            move |args| set_skill_points(tracker, args),
        ),
        OpDef::new(
            "skill_state",
            "Snapshot of RPG state (xp, level, points, per-skill levels)",
            "{}",
            move |_args| skill_state(tracker),
        ),
        OpDef::new(
            "skill_add_xp",
            "Manually award XP (debug)",
            "{amount: u64}",
            move |args| skill_add_xp(tracker, args),
        ),
    ]);
}

pub fn skill_toggle<E: Engine>(tracker: &Tracker<E>, args: &Json) -> Result<Json, String> {
    let id = arg_str(args, "id")?;
    let enabled = arg_bool(args, "enabled")?;
    let skill = tracker
        .catalog()
        .def(id)
        .ok_or_else(|| format!("unknown skill '{id}'"))?;
    let now_disabled = tracker.disabled_skills().set(skill.id, !enabled);
    tracker.reapply_one(skill.id);
    Ok(serde_json::json!({"id": id, "enabled": !now_disabled}))
}

pub fn skill_spend<E: Engine>(tracker: &Tracker<E>, args: &Json) -> Result<Json, String> {
    let id = arg_str(args, "id")?;
    let count = arg_u64(args, "count", Some(1))?;
    let count = u32::try_from(count).map_err(|_| format!("count {count} > u32::MAX"))?;
    if tracker.catalog().def(id).is_none() {
        return Err(format!("unknown skill '{id}'"));
    }
    let spent = tracker.spend_skill_points(id, count);
    Ok(serde_json::json!({"id": id, "requested": count, "spent": spent}))
}

pub fn skill_refund<E: Engine>(tracker: &Tracker<E>, args: &Json) -> Result<Json, String> {
    let id = arg_str(args, "id")?;
    let count = arg_u64(args, "count", Some(1))?;
    let count = u32::try_from(count).map_err(|_| format!("count {count} > u32::MAX"))?;
    if tracker.catalog().def(id).is_none() {
        return Err(format!("unknown skill '{id}'"));
    }
    let refunded = tracker.refund_skill_points(id, count);
    Ok(serde_json::json!({"id": id, "requested": count, "refunded": refunded}))
}

pub fn reload_slot<E: Engine>(tracker: &Tracker<E>) -> Result<Json, String> {
    if tracker.reapply_all() {
        Ok(serde_json::json!({"reapplied": true}))
    } else {
        Err("no slot active; cannot reload".into())
    }
}

pub fn set_skill_points<E: Engine>(tracker: &Tracker<E>, args: &Json) -> Result<Json, String> {
    let count = arg_u64(args, "count", None)?;
    let count = u32::try_from(count).map_err(|_| format!("count {count} > u32::MAX"))?;
    if !tracker.debug_grant_skill_points(count) {
        return Err("no slot active".into());
    }
    Ok(serde_json::json!({"granted": count}))
}

pub fn skill_state<E: Engine>(tracker: &Tracker<E>) -> Result<Json, String> {
    let snapshot = tracker.with_state(|s| {
        let mut skills = serde_json::Map::new();
        for skill in tracker.catalog().iter() {
            let lv = s.level_of(skill.id);
            skills.insert(
                skill.id.to_string(),
                serde_json::json!({
                    "level": lv,
                    "max_level": skill.max_level,
                    "effect": tracker.format_effect(skill, lv),
                }),
            );
        }
        serde_json::json!({
            "xp": s.xp,
            "level": s.level,
            "skill_points": s.skill_points,
            "skills": Json::Object(skills),
        })
    });
    Ok(snapshot.unwrap_or_else(|| serde_json::json!({"active": false, "msg": "no slot active"})))
}

pub fn skill_add_xp<E: Engine>(tracker: &Tracker<E>, args: &Json) -> Result<Json, String> {
    let amount = arg_u64(args, "amount", None)?;
    let Some(result) = tracker.record_xp(amount) else {
        return Err("no slot active or save failed".into());
    };
    Ok(serde_json::json!({
        "awarded": result.awarded,
        "total_xp": result.total_xp,
        "old_level": result.old_level,
        "new_level": result.new_level,
        "points_gained": result.points_gained,
    }))
}

/// Stable op names this module registers. Dispatchers that
/// want to advertise their op list can interleave with their
/// own ops.
pub const OP_NAMES: &[&str] = &[
    "skill_toggle",
    "skill_spend",
    "skill_refund",
    "reload_slot",
    "set_skill_points",
    "skill_state",
    "skill_add_xp",
];
