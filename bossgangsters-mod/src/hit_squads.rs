//! Hit squads: the family comes for the PLAYER.
//!
//! Everything is the game's own machinery, assembled the way
//! `TerritoryClaimController.SpawnWaveEnemies` does it:
//! `FamilyManager.SelectFamilyCharacterCode` picks the look,
//! `FightManager.CreateFighter` builds the fighter for the
//! family, `WeaponManager.GetRandomWeapon` arms it,
//! `FighterHandler.SetTarget(player, force)` starts the fight
//! through the vanilla fight-action path (the same call the
//! damage-retaliation code makes). The squad is teleported to a
//! ring around the player and comes for them.
//!
//! This module ships the `hit_squad` control op first (research:
//! prove the concept live, on demand); the war scheduler that
//! drives it automatically comes once the spawn is proven.

use modforge::ops::{OP_REGISTRY, OpDef};
use serde_json::{Value as Json, json};
use unityforge::main_thread_queue::MAIN_QUEUE;
use unityforge::mono::{MonoObject, MonoType, json_handle, owned_object};

pub fn install() {
    OP_REGISTRY.register(OpDef::new(
        "hit_squad",
        "Spawn a family hit squad near the player that attacks them (family: ViceFamily|KurohanaFamily, size, weapon_tier)",
        "{family: str, size?: int, weapon_tier?: int}",
        hit_squad_op,
    ));
}

fn hit_squad_op(args: &Json) -> Result<Json, String> {
    let family = args
        .get("family")
        .and_then(Json::as_str)
        .ok_or("family required: ViceFamily or KurohanaFamily")?
        .to_string();
    if family != "ViceFamily" && family != "KurohanaFamily" {
        return Err("family must be ViceFamily or KurohanaFamily".into());
    }
    let size = args.get("size").and_then(Json::as_i64).unwrap_or(2).clamp(1, 8);
    let tier = args
        .get("weapon_tier")
        .and_then(Json::as_i64)
        .unwrap_or(1)
        .clamp(0, 4);
    MAIN_QUEUE.run(
        "hit_squad",
        std::time::Duration::from_secs(10),
        move || spawn_squad(&family, size, tier),
    )?
}

fn singleton(type_name: &str) -> Result<MonoObject, String> {
    MonoType::find(type_name)
        .ok_or_else(|| format!("{type_name} type not found"))?
        .singleton_instance()
        .ok_or_else(|| format!("no {type_name} instance"))
}

fn handle_of(json: &Json, what: &str) -> Result<i64, String> {
    json_handle(json)
        .map(|h| h as i64)
        .ok_or_else(|| format!("{what} has no handle"))
}

/// Spawn `size` fighters of `family` in a ring around the player
/// and set the player as their fight target. Main thread only.
fn spawn_squad(family: &str, size: i64, tier: i64) -> Result<Json, String> {
    let family_manager = singleton("FamilyManager")?;
    let fight_manager = singleton("FightManager")?;
    let weapon_manager = singleton("WeaponManager")?;
    let club_player = singleton("ClubPlayer")?;

    let player_json = club_player.read_field("playerFighterHandler")?;
    let player_handle = json_handle(&player_json).ok_or("no playerFighterHandler")?;
    let player = owned_object(player_handle);
    let player_bot = owned_object(json_handle(&player.invoke("GetBot", &json!([]))?).ok_or("no player bot")?);
    let player_transform_json = player_bot.read_field("transform")?;
    let player_transform = owned_object(json_handle(&player_transform_json).ok_or("no player transform")?);
    let pos = player_transform.read_field("position")?;
    let (px, py, pz) = (
        pos.get("x").and_then(Json::as_f64).ok_or("position.x")?,
        pos.get("y").and_then(Json::as_f64).ok_or("position.y")?,
        pos.get("z").and_then(Json::as_f64).ok_or("position.z")?,
    );

    let code_json = family_manager.invoke("SelectFamilyCharacterCode", &json!([family]))?;
    let code = handle_of(&code_json, "character code")?;
    let create_at_json = fight_manager.read_field("transform")?;
    let create_at = handle_of(&create_at_json, "FightManager transform")?;

    let mut spawned = 0;
    for i in 0..size {
        // Ring around the player, ~12 m out: close enough to be a
        // fight, far enough to see them coming.
        let angle = (i as f64) / (size as f64) * std::f64::consts::TAU;
        let (ox, oz) = (angle.cos() * 12.0, angle.sin() * 12.0);

        let fighter_json = fight_manager.invoke(
            "CreateFighter",
            &json!([
                {"$handle": create_at},
                false,
                null,
                null,
                null,
                {"$handle": code},
                false,
                family,
                "HitSquad"
            ]),
        )?;
        let fighter = owned_object(json_handle(&fighter_json).ok_or("CreateFighter returned no handle")?);
        fighter.invoke("SetAbilities", &json!([tier]))?;

        let weapon_json = fight_manager_weapon(&weapon_manager, tier)?;
        if let Some(wh) = json_handle(&weapon_json) {
            let weapon = owned_object(wh);
            let item = owned_object(
                json_handle(&weapon.read_field("itemData")?).ok_or("weapon itemData")?,
            );
            let mode = item.read_field("attackMode")?;
            let mode = mode.as_str().map(str::to_string).unwrap_or_else(|| mode.to_string());
            fighter.invoke("SetWeapon", &json!([{"$handle": wh}, mode, true]))?;
        }

        // Teleport via NavMeshAgent.Warp: writing the transform
        // directly gets snapped back by the agent (measured live:
        // the squad spawned 276 m away and jogged over).
        let bot = owned_object(json_handle(&fighter.invoke("GetBot", &json!([]))?).ok_or("no bot")?);
        let agent = owned_object(
            json_handle(&bot.read_field("NavMeshAgent")?).ok_or("no NavMeshAgent")?,
        );
        agent.invoke("Warp", &json!([{"x": px + ox, "y": py, "z": pz + oz}]))?;

        fighter.invoke("SetTarget", &json!([{"$handle": player_handle}, true, null]))?;
        spawned += 1;
    }

    unityforge::mono::log(
        unityforge::mono::LogLevel::Info,
        &format!("bossgangsters-mod: hit squad spawned: {family} size {spawned} weapon tier {tier}"),
    );
    Ok(json!({"family": family, "spawned": spawned, "weapon_tier": tier}))
}

/// GetRandomWeapon(tier), falling back to the punch.
fn fight_manager_weapon(weapon_manager: &MonoObject, tier: i64) -> Result<Json, String> {
    let w = weapon_manager.invoke("GetRandomWeapon", &json!([tier]))?;
    if json_handle(&w).is_some() {
        return Ok(w);
    }
    weapon_manager.invoke("GetPunch", &json!([]))
}
