//! Research: player movement speed.
//!
//! BP_CharacterComponent_C owns MovementSpeed (0x200, Double) and
//! MaxWalkSpeed (0x278, Double). The engine's own
//! CharacterMovementComponent has MaxWalkSpeed at 0x248 (Float).
//!
//! BP_PlayerInventory_C has a MovementSpeeds map at 0xFE8 and an
//! UpdateMaxMovementSpeed function keyed by CharacterState.
//!
//! Writing MovementSpeed and CMC.MaxWalkSpeed had no visible
//! effect. The game probably reads speed from the inventory
//! component's MovementSpeeds map per state.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod \
//!   --test research_movement -- --ignored --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use modforge::client;
use serde_json::json;

const CHAR_COMP: &str = "BP_CharacterComponent_C";
const PLAYER_INV: &str = "BP_PlayerInventory_C";

const MOVEMENT_SPEED: u64 = 0x200;
const MAX_WALK_SPEED_CC: u64 = 0x278;
const SPRINTING: u64 = 0x19D;
const CHAR_STANCE: u64 = 0x19F;

const USE_HOLDABLE_SPEEDS: u64 = 0x358;
const MOVEMENT_SPEEDS_MAP: u64 = 0xFE8;

#[test]
#[ignore = "needs live game"]
fn read_all_speed_fields() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }

    println!("=== BP_CharacterComponent_C ===");
    let instances = client::walk_class_instances_with_cdo(&api, CHAR_COMP, 100);
    println!("{} instance(s)", instances.len());
    for inst in &instances {
        let ms = client::read_f64(&api, inst.addr, MOVEMENT_SPEED);
        let mws = client::read_f64(&api, inst.addr, MAX_WALK_SPEED_CC);
        let sprint = client::read_u8(&api, inst.addr, SPRINTING);
        let stance = client::read_u8(&api, inst.addr, CHAR_STANCE);
        println!("  {} (cdo={})", inst.full_name, !inst.full_name.contains("PersistentLevel"));
        println!("    MovementSpeed  +0x200 = {ms}");
        println!("    MaxWalkSpeed   +0x278 = {mws}");
        println!("    Sprinting      +0x19D = {sprint}");
        println!("    CharacterStance+0x19F = {stance}");
    }

    println!("\n=== BP_PlayerInventory_C ===");
    let instances2 = client::walk_class_instances_with_cdo(&api, PLAYER_INV, 100);
    println!("{} instance(s)", instances2.len());
    for inst in &instances2 {
        if !inst.full_name.contains("PersistentLevel") { continue; }
        let use_hold = client::read_u8(&api, inst.addr, USE_HOLDABLE_SPEEDS);
        println!("  {}", inst.full_name);
        println!("    UseHoldableMovementSpeeds +0x358 = {use_hold}");

        let map_raw = client::read_bytes(&api, inst.addr, MOVEMENT_SPEEDS_MAP, 80);
        if !map_raw.is_empty() {
            println!("    MovementSpeeds +0xFE8 raw ({} bytes): {}", map_raw.len(), hex::encode(&map_raw));
        }
    }

    println!("\n=== CharacterMovementComponent (player) ===");
    let r3 = api.op("walk_class", json!({"class": "CharacterMovementComponent"}));
    let arr3 = r3.result["instances"].as_array().cloned().unwrap_or_default();
    for inst in &arr3 {
        let name = inst["full_name"].as_str().unwrap_or("");
        if !name.contains("PersistentLevel") || !name.contains("SGKMasterCharacter") { continue; }
        if inst["is_cdo"].as_bool() == Some(true) { continue; }
        let Some(addr_str) = inst["addr"].as_str() else { continue };
        let Ok(addr) = u64::from_str_radix(addr_str.trim_start_matches("0x"), 16) else { continue };

        let mws = client::read_f32(&api, addr, 0x248);
        let mwsc = client::read_f32(&api, addr, 0x24C);
        let max_fly = client::read_f32(&api, addr, 0x250);
        let max_accel = client::read_f32(&api, addr, 0x254);
        let max_custom = client::read_f32(&api, addr, 0x258);

        println!("  {name}");
        println!("    MaxWalkSpeed         +0x248 = {mws}");
        println!("    MaxWalkSpeedCrouched  +0x24C = {mwsc}");
        println!("    MaxFlySpeed          +0x250 = {max_fly}");
        println!("    MaxAcceleration      +0x254 = {max_accel}");
        println!("    MaxCustomMovement    +0x258 = {max_custom}");
    }

    println!("\n=== BP_MasterHoldable_C (holdable speeds) ===");
    let holdables = client::walk_class_instances(&api, "BP_MasterHoldable_C", 100);
    let live: Vec<_> = holdables.iter()
        .filter(|i| i.full_name.contains("PersistentLevel"))
        .collect();
    println!("{} live instance(s)", live.len());
    for inst in live.iter().take(3) {
        let use_hold = client::read_u8(&api, inst.addr, USE_HOLDABLE_SPEEDS);
        println!("  {}", inst.full_name);
        println!("    UseHoldableMovementSpeeds +0x358 = {use_hold}");
    }
}
