//! Standard Unity Effect implementations.
//!
//! Five shapes cover the bulk of Unity RPG skills:
//!
//! - [`UnityFieldAdditiveEffect`]. Capture vanilla on first
//!   apply; write `vanilla + max_bonus * progress`. Use for
//!   stacking-style fields (max health, carry capacity).
//! - [`UnityFieldMultiplyEffect`]. Capture vanilla; write
//!   `vanilla * (1 + max_bonus * progress)`. Use for
//!   multiplicative buffs (damage, speed, drop value).
//! - [`UnityMethodInvokeEffect`]. Fire a method on slot
//!   change. Use when the effect is "tell the game to recompute
//!   X" (e.g. UI refresh, recalculate stats).
//! - [`UnityStaticPropAdditiveEffect`]. Like FieldAdditive but
//!   reads/writes via static `get_X`/`set_X` method pairs
//!   instead of fields on a singleton instance.
//! - [`UnityInstancePropMultiplyEffect`]. Walk instances of a
//!   class, take the first, read/write via instance `get_X`/
//!   `set_X` method pairs. Multiplicative across multiple
//!   properties.
//! - [`UnityGuardedMainThreadEffect`]. Check a caller-supplied
//!   guard, then apply another Unity effect through the main-thread
//!   queue with the caller's label and timeout.
//!
//! Each calls through the runtime-tagged bridge so the same
//! struct works on Mono and IL2CPP without recompilation; the
//! active shim populates the bridge entries.
//!
//! Effects that resolve a `MonoType` cache it in a lazy
//! `OnceLock`, avoiding a full type lookup on every application.

use std::sync::OnceLock;

use modforge::rpg::format;
use modforge::rpg::progress::sqrt_progress;
use modforge::rpg::{Effect, TriggerCtx, vanilla::VanillaCache};
use serde_json::json;

use crate::bridge::MonoHandle;
use crate::main_thread_queue::MAIN_QUEUE;
use crate::mono::{self, MonoObject, MonoType};
use crate::rpg::engine::UnityEngine;

/// Guard and dispatch another Unity effect through the main-thread queue.
///
/// The caller retains the guard, queue label, timeout, and concrete effect;
/// Unityforge owns only the reusable dispatch and delegation.
pub struct UnityGuardedMainThreadEffect<E: Effect<UnityEngine> + Sync> {
    label: &'static str,
    timeout: std::time::Duration,
    enabled: fn() -> bool,
    inner: &'static E,
}

impl<E: Effect<UnityEngine> + Sync> UnityGuardedMainThreadEffect<E> {
    pub const fn new(
        label: &'static str,
        timeout: std::time::Duration,
        enabled: fn() -> bool,
        inner: &'static E,
    ) -> Self {
        Self {
            label,
            timeout,
            enabled,
            inner,
        }
    }
}

impl<E: Effect<UnityEngine> + Sync> Effect<UnityEngine> for UnityGuardedMainThreadEffect<E> {
    fn apply(&self, level: u32, max_level: u32, _ctx: &TriggerCtx<'_, UnityEngine>) {
        if !(self.enabled)() {
            return;
        }
        let inner = self.inner;
        let _ = MAIN_QUEUE.run(self.label, self.timeout, move || {
            inner.apply(level, max_level, &TriggerCtx::SlotChange);
        });
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        self.inner.format(level, max_level)
    }
}

/// `vanilla + max_bonus * progress` on a singleton field.
///
/// Captures the engine's first-seen value into `vanilla` and
/// writes from that baseline thereafter, so toggling /
/// refunding restores the true engine value, not whatever the
/// mod last wrote.
pub struct UnityFieldAdditiveEffect {
    pub class_name: &'static str,
    pub field_name: &'static str,
    pub max_bonus: f32,
    pub format_word: &'static str,
    pub vanilla: &'static VanillaCache<&'static str, f32>,
    pub type_cache: OnceLock<MonoType>,
}

impl UnityFieldAdditiveEffect {
    pub const fn new(
        class_name: &'static str,
        field_name: &'static str,
        max_bonus: f32,
        format_word: &'static str,
        vanilla: &'static VanillaCache<&'static str, f32>,
    ) -> Self {
        Self {
            class_name,
            field_name,
            max_bonus,
            format_word,
            vanilla,
            type_cache: OnceLock::new(),
        }
    }
}

impl Effect<UnityEngine> for UnityFieldAdditiveEffect {
    fn apply(&self, level: u32, max_level: u32, _ctx: &TriggerCtx<'_, UnityEngine>) {
        let progress = sqrt_progress(level, max_level);
        let ty = match self.type_cache.get() {
            Some(t) => t,
            None => {
                let Some(t) = MonoType::find(self.class_name) else {
                    return;
                };
                self.type_cache.get_or_init(|| t)
            }
        };
        let Some(obj) = ty.singleton_instance() else {
            return;
        };
        let cur = match obj
            .read_field(self.field_name)
            .ok()
            .and_then(|v| v.as_f64())
        {
            Some(v) => v as f32,
            None => return,
        };
        let baseline = if cur.is_finite() && cur != 0.0 {
            self.vanilla.get_or_init(self.field_name, cur)
        } else {
            self.vanilla.get(self.field_name).unwrap_or(cur)
        };
        let target = baseline + self.max_bonus * progress;
        let _ = obj.write_field(self.field_name, &json!(target));
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        format::format_additive_f32_as_int(self.max_bonus, level, max_level, self.format_word)
    }
}

/// `vanilla * (1 + max_bonus * progress)` on a singleton field.
pub struct UnityFieldMultiplyEffect {
    pub class_name: &'static str,
    pub field_name: &'static str,
    pub max_bonus: f32,
    pub format_word: &'static str,
    pub vanilla: &'static VanillaCache<&'static str, f32>,
    pub type_cache: OnceLock<MonoType>,
}

impl UnityFieldMultiplyEffect {
    pub const fn new(
        class_name: &'static str,
        field_name: &'static str,
        max_bonus: f32,
        format_word: &'static str,
        vanilla: &'static VanillaCache<&'static str, f32>,
    ) -> Self {
        Self {
            class_name,
            field_name,
            max_bonus,
            format_word,
            vanilla,
            type_cache: OnceLock::new(),
        }
    }
}

impl Effect<UnityEngine> for UnityFieldMultiplyEffect {
    fn apply(&self, level: u32, max_level: u32, _ctx: &TriggerCtx<'_, UnityEngine>) {
        let progress = sqrt_progress(level, max_level);
        let mult = 1.0 + self.max_bonus * progress;
        let ty = match self.type_cache.get() {
            Some(t) => t,
            None => {
                let Some(t) = MonoType::find(self.class_name) else {
                    return;
                };
                self.type_cache.get_or_init(|| t)
            }
        };
        let Some(obj) = ty.singleton_instance() else {
            return;
        };
        let cur = match obj
            .read_field(self.field_name)
            .ok()
            .and_then(|v| v.as_f64())
        {
            Some(v) => v as f32,
            None => return,
        };
        let baseline = if cur.is_finite() && cur != 0.0 {
            self.vanilla.get_or_init(self.field_name, cur)
        } else {
            self.vanilla.get(self.field_name).unwrap_or(cur)
        };
        let target = baseline * mult;
        let _ = obj.write_field(self.field_name, &json!(target));
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        format::format_multiplier(self.max_bonus, level, max_level, self.format_word)
    }
}

/// Invoke a parameterless method on a singleton on slot change.
/// Use when the engine has a "recompute X" function the effect
/// just needs to tell to run.
pub struct UnityMethodInvokeEffect {
    pub class_name: &'static str,
    pub method_name: &'static str,
    pub format_text: &'static str,
    pub type_cache: OnceLock<MonoType>,
}

impl UnityMethodInvokeEffect {
    pub const fn new(
        class_name: &'static str,
        method_name: &'static str,
        format_text: &'static str,
    ) -> Self {
        Self {
            class_name,
            method_name,
            format_text,
            type_cache: OnceLock::new(),
        }
    }
}

impl Effect<UnityEngine> for UnityMethodInvokeEffect {
    fn apply(&self, _level: u32, _max_level: u32, _ctx: &TriggerCtx<'_, UnityEngine>) {
        let ty = match self.type_cache.get() {
            Some(t) => t,
            None => {
                let Some(t) = MonoType::find(self.class_name) else {
                    return;
                };
                self.type_cache.get_or_init(|| t)
            }
        };
        let Some(obj) = ty.singleton_instance() else {
            return;
        };
        let _ = obj.invoke(self.method_name, &json!([]));
    }

    fn format(&self, _level: u32, _max_level: u32) -> String {
        self.format_text.to_string()
    }
}

/// `vanilla + max_bonus * progress` on a static property via
/// `get_X`/`set_X` static method pairs. Use when the game
/// exposes a value as a static property (IL2CPP codegen pattern)
/// rather than a field on a singleton instance.
pub struct UnityStaticPropAdditiveEffect {
    pub class_name: &'static str,
    pub prop_name: &'static str,
    pub max_bonus: f32,
    pub format_word: &'static str,
    pub vanilla: &'static VanillaCache<&'static str, f32>,
}

impl UnityStaticPropAdditiveEffect {
    pub const fn new(
        class_name: &'static str,
        prop_name: &'static str,
        max_bonus: f32,
        format_word: &'static str,
        vanilla: &'static VanillaCache<&'static str, f32>,
    ) -> Self {
        Self {
            class_name,
            prop_name,
            max_bonus,
            format_word,
            vanilla,
        }
    }
}

impl Effect<UnityEngine> for UnityStaticPropAdditiveEffect {
    fn apply(&self, level: u32, max_level: u32, _ctx: &TriggerCtx<'_, UnityEngine>) {
        let progress = sqrt_progress(level, max_level);
        let getter = format!("get_{}", self.prop_name);
        let setter = format!("set_{}", self.prop_name);
        let cur = mono::invoke_static(self.class_name, &getter, &json!([]))
            .ok()
            .and_then(|v| v.as_f64())
            .map(|v| v as f32);
        let Some(cur) = cur else { return };
        let baseline = if cur.is_finite() && cur != 0.0 {
            self.vanilla.get_or_init(self.prop_name, cur)
        } else {
            self.vanilla.get(self.prop_name).unwrap_or(cur)
        };
        let target = baseline + self.max_bonus * progress;
        let _ = mono::invoke_static(self.class_name, &setter, &json!([target]));
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        format::format_additive_f32_as_int(self.max_bonus, level, max_level, self.format_word)
    }
}

/// `vanilla * (1 + max_bonus * progress)` on instance properties
/// of the first live instance of a class, via `get_X`/`set_X`
/// instance method pairs. Use when the target is a component
/// instance (e.g. a player controller) rather than a singleton.
pub struct UnityInstancePropMultiplyEffect {
    pub class_name: &'static str,
    pub prop_names: &'static [&'static str],
    pub max_bonus: f32,
    pub format_word: &'static str,
    pub vanilla: &'static VanillaCache<&'static str, f32>,
    pub type_cache: OnceLock<MonoType>,
}

impl UnityInstancePropMultiplyEffect {
    pub const fn new(
        class_name: &'static str,
        prop_names: &'static [&'static str],
        max_bonus: f32,
        format_word: &'static str,
        vanilla: &'static VanillaCache<&'static str, f32>,
    ) -> Self {
        Self {
            class_name,
            prop_names,
            max_bonus,
            format_word,
            vanilla,
            type_cache: OnceLock::new(),
        }
    }
}

impl Effect<UnityEngine> for UnityInstancePropMultiplyEffect {
    fn apply(&self, level: u32, max_level: u32, _ctx: &TriggerCtx<'_, UnityEngine>) {
        let mult = 1.0 + self.max_bonus * sqrt_progress(level, max_level);
        let ty = match self.type_cache.get() {
            Some(t) => t,
            None => {
                let Some(t) = MonoType::find(self.class_name) else {
                    return;
                };
                self.type_cache.get_or_init(|| t)
            }
        };
        let Ok(walked) = ty.walk(false) else { return };
        let Some(h) = walked
            .as_array()
            .and_then(|a| a.first())
            .and_then(|i| i["handle"].as_i64())
        else {
            return;
        };
        let obj = unsafe { MonoObject::from_handle(MonoHandle(h as i32)) };
        for prop in self.prop_names {
            let getter = format!("get_{prop}");
            let setter = format!("set_{prop}");
            let cur = obj
                .invoke(&getter, &json!([]))
                .ok()
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            let Some(cur) = cur else { continue };
            let baseline = if cur.is_finite() && cur != 0.0 {
                self.vanilla.get_or_init(prop, cur)
            } else {
                self.vanilla.get(prop).unwrap_or(cur)
            };
            let _ = obj.invoke(&setter, &json!([baseline * mult]));
        }
    }

    fn format(&self, level: u32, max_level: u32) -> String {
        format::format_multiplier(self.max_bonus, level, max_level, self.format_word)
    }
}
