//! Fight skill scales the player's movement speed.
//!
//! Vanilla: `PlayerBot.Start` copies `walkSpeed` / `runSpeed`
//! from the `BotManager` singleton (2.8 / 4.5 defaults) and no
//! skill ever changes them. This module multiplies both by
//! `1 + 0.01 x Fight level`: +1% per level, uncapped, so the
//! 100-level cap keeps paying (Fight 100 doubles speed).
//!
//! The speeds are recomputed FROM BASE each apply (idempotent,
//! nothing compounds) at two moments: when the player's bot
//! spawns (prefix on `PlayerBot.Start` queues the apply for the
//! next frame, after the vanilla copy ran) and after any skill
//! level-up (`skill_cap` calls in, cheap and idempotent even
//! when the leveled skill was not Fight).

use std::ffi::c_void;
use std::os::raw::c_char;

use unityforge::hook::{HOOK_REGISTRY, patch_prefix_instance_args};
use unityforge::main_thread_queue::MAIN_QUEUE;
use unityforge::mono::{self, LogLevel, json_handle, owned_object};

/// Speed multiplier gained per Fight level.
const SPEED_PER_FIGHT_LEVEL: f64 = 0.01;

/// FighterSkill.Fight.
const SKILL_FIGHT: i64 = 1;

pub fn install() {
    match patch_prefix_instance_args("PlayerBot", "Start", player_bot_start_prefix) {
        Ok(hook) => {
            HOOK_REGISTRY.register(hook);
            mono::log(
                LogLevel::Info,
                "bossgangsters-mod: fight speed armed (PlayerBot.Start patched)",
            );
        }
        Err(e) => mono::log(
            LogLevel::Error,
            &format!("bossgangsters-mod: fight speed patch failed: {e}"),
        ),
    }
}

extern "C" fn player_bot_start_prefix(instance: *const c_void, _args: *const c_char) -> i32 {
    if !instance.is_null() {
        // Release the instance handle we own; apply() finds the
        // player through ClubPlayer itself.
        let _instance = owned_object(instance as i32);
        // Vanilla Start runs after this prefix and overwrites the
        // speeds; apply on the next frame's drain, after it ran.
        MAIN_QUEUE.push(|| {
            if let Err(e) = apply() {
                mono::log(
                    LogLevel::Warn,
                    &format!("bossgangsters-mod: fight speed not applied: {e}"),
                );
            }
        });
    }
    0 // always run vanilla Start
}

/// Recompute the player's walk and run speed from the BotManager
/// base and the current Fight level. Main thread only.
pub fn apply() -> Result<(), String> {
    let club_player = unityforge::mono::MonoType::find("ClubPlayer")
        .ok_or("ClubPlayer type not found")?
        .singleton_instance()
        .ok_or("no ClubPlayer instance")?;
    let fighter_json = club_player.read_field("playerFighterHandler")?;
    let fighter = owned_object(json_handle(&fighter_json).ok_or("no playerFighterHandler")?);
    let bot_json = fighter.invoke("GetBot", &serde_json::json!([]))?;
    let bot = owned_object(json_handle(&bot_json).ok_or("GetBot returned no handle")?);

    let bot_manager = unityforge::mono::MonoType::find("BotManager")
        .ok_or("BotManager type not found")?
        .singleton_instance()
        .ok_or("no BotManager instance")?;
    let base_walk = bot_manager
        .invoke("FighterWalkSpeed", &serde_json::json!([]))?
        .as_f64()
        .ok_or("FighterWalkSpeed not a number")?;
    let base_run = bot_manager
        .invoke("FighterRunSpeed", &serde_json::json!([]))?
        .as_f64()
        .ok_or("FighterRunSpeed not a number")?;

    let fight = fighter
        .invoke("GetSkillLevel", &serde_json::json!([SKILL_FIGHT]))?
        .as_i64()
        .ok_or("GetSkillLevel not an int")?;
    let mult = 1.0 + SPEED_PER_FIGHT_LEVEL * fight as f64;

    bot.write_field("walkSpeed", &serde_json::json!(base_walk * mult))?;
    bot.write_field("runSpeed", &serde_json::json!(base_run * mult))?;
    mono::log(
        LogLevel::Info,
        &format!(
            "bossgangsters-mod: fight speed applied: Fight {fight} -> x{mult:.2} (walk {:.2}, run {:.2})",
            base_walk * mult,
            base_run * mult
        ),
    );
    Ok(())
}
