//! Unity-side RPG framework. Plugs the modforge engine-generic
//! traits into the Unity runtime via the bridge.
//!
//! Type aliases here pin `modforge::rpg`'s engine type
//! parameter to [`UnityEngine`] so catalog rows don't have to
//! name it.

pub mod engine;
pub mod ops;
pub mod skill;
pub mod slot_key;
pub mod std_effect;
pub mod tracker;
pub mod trigger_harmony;

pub use engine::{UnityEngine, UnityEvent};
pub use skill::{SkillDef, SkillRegistry};
pub use slot_key::UnitySlotKey;
pub use std_effect::{
    UnityFieldAdditiveEffect, UnityFieldMultiplyEffect, UnityInstancePropMultiplyEffect,
    UnityMethodInvokeEffect, UnityStaticPropAdditiveEffect,
};
pub use tracker::Tracker;
pub use trigger_harmony::{
    HarmonyPostfixTrigger, HarmonyPrefixTrigger, UnityEventTrigger, ON_HARMONY_POST,
    ON_HARMONY_PRE, ON_UNITY_EVENT,
};
