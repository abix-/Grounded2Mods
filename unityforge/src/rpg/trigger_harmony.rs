//! Harmony-driven triggers for the Unity RPG framework.
//!
//! Two trigger kinds + their static `TriggerDef`s:
//!
//! - `"OnHarmonyPost"` via [`ON_HARMONY_POST`]. Catalog rows
//!   use this trigger when they want to react to a method
//!   that the mod patches with a Harmony postfix.
//! - `"OnHarmonyPre"` via [`ON_HARMONY_PRE`]. Same but for
//!   prefix patches.
//!
//! Patch installation is the game crate's responsibility (it
//! has to name the `(class, method)` target and the
//! `extern "C"` trampoline body). The trampoline decodes the
//! patched method's arguments and calls [`fire_post`] /
//! [`fire_pre`] to push a [`UnityEvent`] through the
//! [`Tracker::fire`] surface.
//!
//! ```ignore
//! extern "C" fn on_dig_postfix(_ctx: *const c_void) {
//!     unityforge::rpg::trigger_harmony::fire_post(
//!         &TRACKER, "DigManager", "Dig", "{}", "null",
//!     );
//! }
//!
//! patch_postfix("DigManager", "Dig", on_dig_postfix)
//!     .map(|hook| HOOK_REGISTRY.register(hook));
//! ```
//!
//! All catalog skills whose `trigger.kind == "OnHarmonyPost"`
//! fire on every postfix dispatch through `fire_post`; per-
//! skill match-by-class/method happens inside the effect's
//! `apply`.

use modforge::rpg::{Trigger, TriggerCtx, TriggerDef};

use crate::rpg::engine::UnityEvent;
use crate::rpg::tracker::Tracker;

/// Marker trigger for Harmony postfix fires. The framework's
/// `Tracker::fire` matches by kind string; per-skill filtering
/// on `class` / `method` happens inside the effect's apply
/// (the effect pattern-matches on
/// [`UnityEvent::HarmonyPost { class, method, .. }`]).
pub struct HarmonyPostfixTrigger;
impl Trigger for HarmonyPostfixTrigger {
    fn kind(&self) -> &'static str {
        "OnHarmonyPost"
    }
}
static ON_HARMONY_POST_IMP: HarmonyPostfixTrigger = HarmonyPostfixTrigger;
pub static ON_HARMONY_POST: TriggerDef = TriggerDef::new("OnHarmonyPost", &ON_HARMONY_POST_IMP);

/// Marker trigger for Harmony prefix fires.
pub struct HarmonyPrefixTrigger;
impl Trigger for HarmonyPrefixTrigger {
    fn kind(&self) -> &'static str {
        "OnHarmonyPre"
    }
}
static ON_HARMONY_PRE_IMP: HarmonyPrefixTrigger = HarmonyPrefixTrigger;
pub static ON_HARMONY_PRE: TriggerDef = TriggerDef::new("OnHarmonyPre", &ON_HARMONY_PRE_IMP);

/// Marker trigger for `static event Action` subscriptions
/// (`OnUnityEvent`). Game crates that subscribe to a
/// vanilla-game event use this kind and push fires through
/// [`fire_event`] from their event-subscription closure.
pub struct UnityEventTrigger;
impl Trigger for UnityEventTrigger {
    fn kind(&self) -> &'static str {
        "OnUnityEvent"
    }
}
static ON_UNITY_EVENT_IMP: UnityEventTrigger = UnityEventTrigger;
pub static ON_UNITY_EVENT: TriggerDef = TriggerDef::new("OnUnityEvent", &ON_UNITY_EVENT_IMP);

/// Fire an `OnHarmonyPost` event through the tracker. Called
/// from a game-side trampoline's `extern "C"` body once it has
/// decoded the patched method's arguments.
pub fn fire_post(
    tracker: &Tracker,
    class: &'static str,
    method: &'static str,
    args_json: &str,
    result_json: &str,
) {
    tracker.fire(&TriggerCtx::Engine(UnityEvent::HarmonyPost {
        class,
        method,
        args_json,
        result_json,
    }));
}

/// Fire an `OnHarmonyPre` event through the tracker.
pub fn fire_pre(tracker: &Tracker, class: &'static str, method: &'static str, args_json: &str) {
    tracker.fire(&TriggerCtx::Engine(UnityEvent::HarmonyPre {
        class,
        method,
        args_json,
    }));
}

/// Fire an `OnUnityEvent` through the tracker.
pub fn fire_event(tracker: &Tracker, source: &'static str, payload_json: &str) {
    tracker.fire(&TriggerCtx::Engine(UnityEvent::UnityEvent {
        source,
        payload_json,
    }));
}
