//! HealthComponent simulation ops -- `simulate_add_health` and
//! `simulate_set_current_health`.
//!
//! ```text
//! K8s slot: Def=HealthBinding (game-supplied constants),
//!           Registry=ueforge::ops::OP_REGISTRY,
//!           Instance=per-call PE queue job,
//!           Controller=register() + per-op closure
//! ```
//!
//! Every UE5 RPG / survival game has a `UHealthComponent` (or
//! analogue) with a single `f32` "current damage" field plus a
//! handful of UFunctions that mutate it (`AddHealth`,
//! `SetCurrentHealth`, `ApplyDamageFromInfo`). The op handlers
//! that simulate those calls are universal in **shape**:
//!
//! 1. resolve a selector to the live HC instance,
//! 2. read the `current_damage` field for before/after telemetry,
//! 3. enqueue a PE call to the game-thread drain,
//! 4. inside the drain, build a parm struct + invoke the
//!    UFunction via `pe_call::call_ufunction`,
//! 5. read the after-value, return JSON with the deltas.
//!
//! The **constants**. HC class name, selector name, field
//! offsets, UFunction names. Are game-specific. Game crates
//! declare a [`HealthBinding`] with their values and call
//! [`register`] once at worker init. ueforge handles the rest.
//!
//! ```ignore
//! static HEALTH_CLASS: ueforge::ue::ClassRef = ueforge::ue::ClassRef::new("HealthComponent");
//!
//! static HEALTH_BINDING: ueforge::rpg::health::HealthBinding =
//!     ueforge::rpg::health::HealthBinding {
//!         hc_class: &HEALTH_CLASS,
//!         hc_selector: "live_player_hc",
//!         current_damage_offset: 0x032C,
//!         max_health_offset: 0x0328,
//!         add_health_function: Some("AddHealth"),
//!         set_current_health_function: Some("SetCurrentHealth"),
//!     };
//!
//! // From worker():
//! ueforge::rpg::health::register(&HEALTH_BINDING, &PE_QUEUE, PE_TIMEOUT_HINT);
//! ```
//!
//! ## Parm shapes
//!
//! The Maine SDK (Grounded 2) ships these UFunctions with parm
//! shapes that match every UE5 RPG HC we've inspected:
//!
//! - **`AddHealth(amount: f32, causer: *mut UObject)`**. 16-byte
//!   parm. `causer` may be null.
//! - **`SetCurrentHealth(desired_health: f32)`**. 4-byte parm.
//!
//! If a future game's HC exposes a different signature, generalise
//! the parm builder via a trait or callback. v1 hardcodes
//! Maine-canonical shapes.
//!
//! ## What is NOT shipped here
//!
//! `simulate_apply_damage`. The parm shape is `0x100` bytes with
//! an embedded `FDamageInfo`. Can re-enter `ProcessEvent` from
//! the engine's damage-replication path and crash the host. The
//! safe-drain-site work (Wave E1: global `ProcessEvent`
//! pre-callback) gates it. Until then, games that want the op
//! ship their own stub returning a "disabled, see ueforge docs"
//! error so tests can detect-and-skip.

use std::ffi::c_void;
use std::time::Duration;

use serde_json::Value as Json;

use crate::args::arg_f64;
use crate::ops::{OP_REGISTRY, OpDef};
use crate::pe_queue::GameThread;
use crate::ue::ClassRef;

/// Game-supplied constants for the health-op family. See module
/// docs for an example declaration.
pub struct HealthBinding {
    /// `ClassRef` for the HC class. ueforge looks up the
    /// UFunctions through this.
    pub hc_class: &'static ClassRef,
    /// Name of a registered selector that resolves to a live HC
    /// instance. Typically `"live_player_hc"`. Resolved through
    /// [`crate::selector::resolve`] each op call.
    pub hc_selector: &'static str,
    /// Byte offset of the `f32` "current damage" field on the HC
    /// instance. The framework reads it before + after every op
    /// for the response JSON.
    pub current_damage_offset: usize,
    /// Byte offset of the `f32` "max health" field. Surfaced in
    /// telemetry; not strictly required for the ops.
    pub max_health_offset: usize,
    /// UFunction name for `AddHealth(amount, causer)`. `None`
    /// skips registration.
    pub add_health_function: Option<&'static str>,
    /// UFunction name for `SetCurrentHealth(desired_health)`.
    /// `None` skips registration.
    pub set_current_health_function: Option<&'static str>,
}

/// Register the health-op family against the workspace
/// [`OP_REGISTRY`] using the game's `HealthBinding`. Each op is
/// registered only when the binding's corresponding function name
/// is `Some`.
///
/// `pe_queue` is the game-thread drain site the closures enqueue
/// onto; `pe_timeout_hint` is appended to timeout errors so users
/// know which trampoline owns the drain.
pub fn register(
    binding: &'static HealthBinding,
    pe_queue: &'static GameThread,
    pe_timeout_hint: &'static str,
) {
    if let Some(func_name) = binding.add_health_function {
        OP_REGISTRY.register(OpDef::new(
            "simulate_add_health",
            "Heal the live HC by `amount` HP via the PE queue",
            "{amount: f32}",
            move |args| op_add_health(args, binding, pe_queue, pe_timeout_hint, func_name),
        ));
    }
    if let Some(func_name) = binding.set_current_health_function {
        OP_REGISTRY.register(OpDef::new(
            "simulate_set_current_health",
            "Force the live HC's CurrentHealth to `value`",
            "{value: f32}",
            move |args| {
                op_set_current_health(args, binding, pe_queue, pe_timeout_hint, func_name)
            },
        ));
    }
}

#[repr(C)]
struct AddHealthParms {
    amount: f32,
    _pad: u32,
    causer: *mut c_void,
}

#[repr(C)]
struct SetCurrentHealthParms {
    desired_health: f32,
}

fn op_add_health(
    args: &Json,
    binding: &'static HealthBinding,
    pe_queue: &'static GameThread,
    hint: &'static str,
    func_name: &'static str,
) -> Result<Json, String> {
    let amount = arg_f64(args, "amount")? as f32;
    crate::debug::enqueue_pe(pe_queue, Duration::from_secs(5), hint, move || {
        let hc = crate::selector::resolve(binding.hc_selector)?;
        let before = crate::ue::field::read_f32(hc, binding.current_damage_offset);
        let mut parms = AddHealthParms {
            amount,
            _pad: 0,
            causer: std::ptr::null_mut(),
        };
        // SAFETY: hc is a live UObject resolved through the registered
        // selector. parms layout matches the Maine-canonical AddHealth
        // signature documented at the module level.
        unsafe {
            crate::ue::pe_call::call_ufunction(hc, binding.hc_class, func_name, &mut parms)?;
        }
        let after = crate::ue::field::read_f32(hc, binding.current_damage_offset);
        Ok(serde_json::json!({
            "amount_requested": amount,
            "current_damage_before": before,
            "current_damage_after": after,
            "delta": before - after,
        }))
    })
}

fn op_set_current_health(
    args: &Json,
    binding: &'static HealthBinding,
    pe_queue: &'static GameThread,
    hint: &'static str,
    func_name: &'static str,
) -> Result<Json, String> {
    let value = arg_f64(args, "value")? as f32;
    crate::debug::enqueue_pe(pe_queue, Duration::from_secs(5), hint, move || {
        let hc = crate::selector::resolve(binding.hc_selector)?;
        let before = crate::ue::field::read_f32(hc, binding.current_damage_offset);
        let max = crate::ue::field::read_f32(hc, binding.max_health_offset);
        let mut parms = SetCurrentHealthParms {
            desired_health: value,
        };
        // SAFETY: see op_add_health. Same selector + ClassRef invariants.
        unsafe {
            crate::ue::pe_call::call_ufunction(hc, binding.hc_class, func_name, &mut parms)?;
        }
        let after = crate::ue::field::read_f32(hc, binding.current_damage_offset);
        Ok(serde_json::json!({
            "value_requested": value,
            "max_health": max,
            "current_damage_before": before,
            "current_damage_after": after,
        }))
    })
}
