//! Raise the skill level cap from 10 to 100.
//!
//! Vanilla `FighterHandler.AddSkillXp` refuses XP once
//! `abilityValue >= 10`. One Harmony prefix replaces it with the
//! same logic (read from the decompile, docs/skills.md) and the
//! cap at 100. Everything downstream is uncapped already: the XP
//! curve is `50 + level * 50`, health is `default + Power * 20`,
//! stamina is `100 + Power * 10`.
//!
//! The prefix runs on the game thread inside the patched call,
//! so bridge reads and invokes here are synchronous, not queued.
//! On ANY failure it returns 0 so the vanilla method runs and the
//! game behaves exactly as unmodded.

use std::ffi::{CStr, c_void};
use std::os::raw::c_char;

use unityforge::hook::{HOOK_REGISTRY, patch_prefix_instance_args};
use unityforge::mono::{self, LogLevel, json_handle, owned_object};

const SKILL_LEVEL_CAP: i64 = 100;

/// FighterSkill.Power, the one skill with side effects on level.
const SKILL_POWER: i64 = 0;

pub fn install() {
    match patch_prefix_instance_args("FighterHandler", "AddSkillXp", add_skill_xp_prefix) {
        Ok(hook) => {
            HOOK_REGISTRY.register(hook);
            mono::log(
                LogLevel::Info,
                "bossgangsters-mod: skill cap raised to 100 (AddSkillXp patched)",
            );
        }
        Err(e) => mono::log(
            LogLevel::Error,
            &format!("bossgangsters-mod: skill cap patch failed: {e}"),
        ),
    }
}

extern "C" fn add_skill_xp_prefix(instance: *const c_void, args_json: *const c_char) -> i32 {
    // 1 = skip the original (we did the work), 0 = run vanilla.
    match add_skill_xp(instance, args_json) {
        Ok(()) => 1,
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("bossgangsters-mod: AddSkillXp prefix fell back to vanilla: {e}"),
            );
            0
        }
    }
}

/// The vanilla method with the cap at 100:
/// ```csharp
/// if (!IsOurFighter || abilityValue >= 10) return;
/// xp *= GetSkillExperienceMultiplier(skill);
/// progressValue += xp;
/// if (progressValue >= GetNextLevelRequired()) {
///     progressValue -= required; abilityValue++;
///     if (skill == Power) { SetMaxHealth(default + Power*20); ClampStaminaToMax(); }
///     LevelAnimation(abilityKey);
/// }
/// ```
fn add_skill_xp(instance: *const c_void, args_json: *const c_char) -> Result<(), String> {
    if instance.is_null() || args_json.is_null() {
        return Err("null instance or args".into());
    }
    let args: serde_json::Value = {
        let raw = unsafe { CStr::from_ptr(args_json) }
            .to_str()
            .map_err(|e| format!("args not utf8: {e}"))?;
        serde_json::from_str(raw).map_err(|e| format!("args not json: {e}"))?
    };
    let skill = args
        .get(0)
        .and_then(|v| v.as_i64())
        .ok_or("args[0] (skill) not an int")?;
    let xp = args
        .get(1)
        .and_then(|v| v.as_f64())
        .ok_or("args[1] (xp) not a number")?;

    let fighter = owned_object(instance as i32);

    let is_our = fighter
        .read_field("IsOurFighter")?
        .as_bool()
        .ok_or("IsOurFighter not a bool")?;
    if !is_our {
        return Ok(()); // vanilla no-op
    }

    let fighter_data_json = fighter.read_field("fighterData")?;
    let fighter_data =
        owned_object(json_handle(&fighter_data_json).ok_or("fighterData has no handle")?);
    let abilities_json = fighter_data.read_field("employeeAbilities")?;
    let abilities =
        owned_object(json_handle(&abilities_json).ok_or("employeeAbilities has no handle")?);
    let ability_json = abilities.invoke("GetValue", &serde_json::json!([skill]))?;
    let ability = owned_object(json_handle(&ability_json).ok_or("ability has no handle")?);

    let level = ability
        .read_field("abilityValue")?
        .as_i64()
        .ok_or("abilityValue not an int")?;
    if level >= SKILL_LEVEL_CAP {
        return Ok(()); // vanilla no-op shape, higher cap
    }

    let mult = fighter
        .invoke("GetSkillExperienceMultiplier", &serde_json::json!([skill]))?
        .as_i64()
        .ok_or("GetSkillExperienceMultiplier not an int")? as f64;
    let mut progress = ability
        .read_field("progressValue")?
        .as_f64()
        .ok_or("progressValue not a number")?
        + xp * mult;

    let required = (50 + level * 50) as f64; // EmployeeAbility.GetNextLevelRequired
    if progress >= required {
        progress -= required;
        let new_level = level + 1;
        ability.write_field("abilityValue", &serde_json::json!(new_level))?;
        ability.write_field("progressValue", &serde_json::json!(progress))?;

        if skill == SKILL_POWER {
            let default_health = fighter
                .read_field("defaultHealth")?
                .as_f64()
                .ok_or("defaultHealth not a number")?;
            let max_health = default_health + (new_level as f64) * 20.0;
            let health_json = fighter.read_field("Health")?;
            let health = owned_object(json_handle(&health_json).ok_or("Health has no handle")?);
            health.invoke("SetMaxHealth", &serde_json::json!([max_health]))?;
            fighter.invoke("ClampStaminaToMax", &serde_json::json!([]))?;
        }

        let key = ability.read_field("abilityKey")?;
        let key = key.as_str().ok_or("abilityKey not a string")?;
        fighter.invoke("LevelAnimation", &serde_json::json!([key]))?;

        // Reapply the Fight speed multiplier on any level-up
        // (idempotent; recomputed from base).
        if let Err(e) = crate::fight_speed::apply() {
            mono::log(
                LogLevel::Warn,
                &format!("bossgangsters-mod: fight speed reapply failed: {e}"),
            );
        }
    } else {
        ability.write_field("progressValue", &serde_json::json!(progress))?;
    }
    Ok(())
}
