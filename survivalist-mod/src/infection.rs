//! No infections. Operator directive 2026-07-04: infection
//! chance is 0%, not a longer death timer. Characters must never
//! carry infected injuries at all.
//!
//! Mechanism (from the decompiled game; see docs/research.md):
//! every injury that can infect a character funnels through
//! `Character.AddInjury(Injury)`: zombie bites and infected
//! melee/ammo (`OnDamaged` -> `new Injury(...)` -> `AddInjury`)
//! and swallowed infected food/liquid (a Swallowed-type injury
//! through the same call). `AddInjury` itself bumps
//! `InfectionProgression` when the injury arrives infected, and
//! any progression above zero can zombify the corpse later, so
//! the fix must run BEFORE the body: a prefix on `AddInjury`
//! zeroes the injury's `InfectionType` + `OriginalInfectionType`
//! (arg0 via the v5 prefix_ctx hook), then lets the original run.
//! The body then sees an uninfected injury: no progression, no
//! infection icon, no antigen-seeking, no infection death.
//!
//! Fire/armor injury paths already carry InfectionType.None.
//! Known secondary vector NOT covered here: `Injury.ApplyBandage`
//! can re-infect an EXISTING injury from an infected bandage
//! (max(current, bandage)); with every injury entering
//! uninfected, max() keeps it None only if the bandage itself is
//! clean. Revisit if infected bandages show up in play.

use std::ffi::c_void;

use serde_json::json;

use unityforge::bridge::MonoHandle;
use unityforge::hook::{self, HOOK_REGISTRY, HookCtx};
use unityforge::mono::{self, LogLevel, MonoObject};

pub fn install() {
    match hook::patch_prefix_ctx("Character", "AddInjury", HookCtx::Arg0, zero_injury_infection) {
        Ok(h) => {
            HOOK_REGISTRY.register(h);
            mono::log(
                LogLevel::Info,
                "survivalist-mod: infection off (Character.AddInjury prefix installed)",
            );
        }
        Err(e) => {
            mono::log(
                LogLevel::Error,
                &format!("survivalist-mod: infection patch FAILED: {e}"),
            );
        }
    }
}

extern "C" fn zero_injury_infection(ctx: *const c_void) -> i32 {
    let handle = ctx as isize as i32;
    if handle != 0 {
        // SAFETY: the shim's prefix_ctx dispatcher acquired this
        // handle for us and we own it; MonoObject's Drop releases
        // it after the writes.
        let injury = unsafe { MonoObject::from_handle(MonoHandle(handle)) };
        // Enum fields are written by variant name (the shim's
        // WriteField does Enum.Parse).
        if let Err(e) = injury.write_field("InfectionType", &json!("None")) {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: infection zero failed: {e}"),
            );
        }
        if let Err(e) = injury.write_field("OriginalInfectionType", &json!("None")) {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: infection zero (original) failed: {e}"),
            );
        }
    }
    0 // run the original AddInjury; it now sees an uninfected injury
}
