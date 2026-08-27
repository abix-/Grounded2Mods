//! The storyteller: the director above the base simulation
//! (docs/status.md "Storyteller / director").
//!
//! Delegates to [`modforge::storyteller::Director`] for the
//! tick-driven weighted-random event pacer; this module wires
//! the game-specific rules and the status op that reads
//! survivalist-specific live state.

use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use modforge::storyteller::{Config, Director};
use unityforge::main_thread_queue::MAIN_QUEUE;
use unityforge::mono::{self, LogLevel};

use crate::common::{session_seed, with};

static RULES: &[Rule] = &[
    crate::horde::RULE,
    crate::vendor::RULE,
    crate::incursion::RULE,
];

static DIRECTOR: Director = Director::with_config(
    RULES,
    Config {
        min_gap_secs: 180.0,
        max_gap_secs: 600.0,
        retry_gap_secs: 60.0,
    },
);

pub use modforge::storyteller::{Outcome, Rule};

/// Advance this system when its scheduled game update is due.
/// Stays here because it applies Survivalist's storyteller pacing rules through the game's classes, fields, content, and actions.
pub fn tick(now: f32) {
    DIRECTOR.tick(
        now,
        || session_seed(),
        |rule, err| {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: storyteller -- rule {rule} failed: {err}"),
            );
        },
    );
}

/// The brutal-but-survivable line for a pressure event: is it safe
/// to lean on this camp? False once it is already handling more
/// threats than the configured ceiling.
/// Stays here because it applies Survivalist's storyteller pacing rules through the game's classes, fields, content, and actions.
pub fn safe_to_pressure(community_h: i32) -> bool {
    let max = guard_max_threats();
    with(community_h, |com| com.field_list_len("Threats") <= max)
}

/// Game-specific guard knob, stored alongside the director config.
static GUARD_MAX_THREATS: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// Read the live ceiling that prevents piling pressure onto a camp already in danger.
/// Stays here because it applies Survivalist's storyteller pacing rules through the game's classes, fields, content, and actions.
fn guard_max_threats() -> i64 {
    GUARD_MAX_THREATS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Expose this system status and controls through the mod control endpoint.
/// Stays here because it applies Survivalist's storyteller pacing rules through the game's classes, fields, content, and actions.
pub fn register_ops() {
    DIRECTOR.register_config_op();
    OP_REGISTRY.register_many([
        OpDef::new(
            "storyteller_status",
            "The director's live state: active storyteller, pacing knobs, seconds until the next event, the last event fired, live director packs, and the current alpha it is watching.",
            "{}",
            storyteller_status,
        ),
        OpDef::new(
            "storyteller_guard",
            "Read or set the guard_max_threats ceiling.",
            "{guard_max_threats?: number}",
            storyteller_guard,
        ),
    ]);
}

/// Report the current warning, horde, vendor, and stranger activity.
/// Stays here because it applies Survivalist's storyteller pacing rules through the game's classes, fields, content, and actions.
fn storyteller_status(_args: &Json) -> Result<Json, String> {
    MAIN_QUEUE.run_result(
        "storyteller_status",
        std::time::Duration::from_secs(5),
        || {
            let mut status = DIRECTOR.status();
            let alpha = crate::horde::alpha_view()?
                .map(|(name, members)| json!({"name": name, "members": members}));
            if let Some(obj) = status.as_object_mut() {
                obj.insert("guard_max_threats".to_string(), json!(guard_max_threats()));
                obj.insert(
                    "packs_live".to_string(),
                    json!(crate::horde::live_pack_count()),
                );
                obj.insert(
                    "vendors_live".to_string(),
                    json!(crate::vendor::active_count()),
                );
                obj.insert(
                    "strangers_live".to_string(),
                    json!(crate::stranger::active_count()),
                );
                obj.insert(
                    "settlers_live".to_string(),
                    json!(crate::settler::active_count()),
                );
                obj.insert(
                    "incursion_pending".to_string(),
                    json!(crate::incursion::pending()),
                );
                obj.insert("alpha".to_string(), alpha.unwrap_or(Json::Null));
            }
            Ok(status)
        },
    )
}

/// Read or change the maximum threats allowed before pressure pauses.
/// Stays here because it applies Survivalist's storyteller pacing rules through the game's classes, fields, content, and actions.
fn storyteller_guard(args: &Json) -> Result<Json, String> {
    if let Some(v) = args.get("guard_max_threats").and_then(Json::as_i64) {
        GUARD_MAX_THREATS.store(v, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(json!({"guard_max_threats": guard_max_threats()}))
}
