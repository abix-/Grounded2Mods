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
use common::{api_or_skip, as_f64, offsets_live, read_bytes, selector_of};
use serde_json::json;

const CHAR_COMP: &str = "BP_CharacterComponent_C";
const PLAYER_INV: &str = "BP_PlayerInventory_C";

const MOVEMENT_SPEED: u64 = 0x200;
const MAX_WALK_SPEED_CC: u64 = 0x278;
const SPRINTING: u64 = 0x19D;
const CHAR_STANCE: u64 = 0x19F;

const USE_HOLDABLE_SPEEDS: u64 = 0x358;
const MOVEMENT_SPEEDS_MAP: u64 = 0xFE8;

fn live_instance(api: &common::Api, class: &str) -> Option<(String, serde_json::Value)> {
    let r = api.op("walk_class", json!({"class": class}));
    if !r.ok { return None; }
    let arr = r.result["instances"].as_array()?.clone();
    arr.into_iter()
        .find(|i| {
            let name = i["full_name"].as_str().unwrap_or("");
            name.contains("PersistentLevel") && i["is_cdo"].as_bool() != Some(true)
        })
        .and_then(|i| {
            let sel = selector_of(&i)?;
            Some((sel, i))
        })
}

#[test]
#[ignore = "needs live game"]
fn read_all_speed_fields() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }

    // BP_CharacterComponent_C
    println!("=== BP_CharacterComponent_C ===");
    let r = api.op("walk_class", json!({"class": CHAR_COMP}));
    let arr = r.result["instances"].as_array().cloned().unwrap_or_default();
    println!("{} instance(s)", arr.len());
    for inst in &arr {
        let name = inst["full_name"].as_str().unwrap_or("?");
        let is_cdo = inst["is_cdo"].as_bool() == Some(true);
        let Some(sel) = selector_of(inst) else { continue };
        let ms = read_bytes(&api, &sel, MOVEMENT_SPEED, 8).and_then(|b| as_f64(&b));
        let mws = read_bytes(&api, &sel, MAX_WALK_SPEED_CC, 8).and_then(|b| as_f64(&b));
        let sprint = read_bytes(&api, &sel, SPRINTING, 1).map(|b| b[0]);
        let stance = read_bytes(&api, &sel, CHAR_STANCE, 1).map(|b| b[0]);
        println!("  {name} (cdo={is_cdo})");
        println!("    MovementSpeed  +0x200 = {ms:?}");
        println!("    MaxWalkSpeed   +0x278 = {mws:?}");
        println!("    Sprinting      +0x19D = {sprint:?}");
        println!("    CharacterStance+0x19F = {stance:?}");
    }

    // BP_PlayerInventory_C
    println!("\n=== BP_PlayerInventory_C ===");
    let r2 = api.op("walk_class", json!({"class": PLAYER_INV}));
    let arr2 = r2.result["instances"].as_array().cloned().unwrap_or_default();
    println!("{} instance(s)", arr2.len());
    for inst in &arr2 {
        let name = inst["full_name"].as_str().unwrap_or("?");
        let is_cdo = inst["is_cdo"].as_bool() == Some(true);
        if !name.contains("PersistentLevel") && !is_cdo { continue; }
        let Some(sel) = selector_of(inst) else { continue };

        let use_hold = read_bytes(&api, &sel, USE_HOLDABLE_SPEEDS, 1).map(|b| b[0]);
        println!("  {name} (cdo={is_cdo})");
        println!("    UseHoldableMovementSpeeds +0x358 = {use_hold:?}");

        // Read raw bytes around MovementSpeeds map to see the TMap header
        let map_raw = read_bytes(&api, &sel, MOVEMENT_SPEEDS_MAP, 80);
        if let Some(raw) = &map_raw {
            println!("    MovementSpeeds +0xFE8 raw ({} bytes): {}", raw.len(), hex::encode(raw));
        }
    }

    // Engine CMC (player only)
    println!("\n=== CharacterMovementComponent (player) ===");
    let r3 = api.op("walk_class", json!({"class": "CharacterMovementComponent"}));
    let arr3 = r3.result["instances"].as_array().cloned().unwrap_or_default();
    for inst in &arr3 {
        let name = inst["full_name"].as_str().unwrap_or("");
        if !name.contains("PersistentLevel") || !name.contains("SGKMasterCharacter") { continue; }
        if inst["is_cdo"].as_bool() == Some(true) { continue; }
        let Some(sel) = selector_of(inst) else { continue };

        let as_f32 = |b: &[u8]| -> f32 {
            f32::from_le_bytes([b[0], b[1], b[2], b[3]])
        };
        let mws = read_bytes(&api, &sel, 0x248, 4).map(|b| as_f32(&b));
        let mwsc = read_bytes(&api, &sel, 0x24C, 4).map(|b| as_f32(&b));
        let max_accel = read_bytes(&api, &sel, 0x254, 4).map(|b| as_f32(&b));
        let max_fly = read_bytes(&api, &sel, 0x250, 4).map(|b| as_f32(&b));
        let max_custom = read_bytes(&api, &sel, 0x258, 4).map(|b| as_f32(&b));

        println!("  {name}");
        println!("    MaxWalkSpeed         +0x248 = {mws:?}");
        println!("    MaxWalkSpeedCrouched  +0x24C = {mwsc:?}");
        println!("    MaxFlySpeed          +0x250 = {max_fly:?}");
        println!("    MaxAcceleration      +0x254 = {max_accel:?}");
        println!("    MaxCustomMovement    +0x258 = {max_custom:?}");
    }

    // BP_MasterHoldable_C
    println!("\n=== BP_MasterHoldable_C (holdable speeds) ===");
    let r4 = api.op("walk_class", json!({"class": "BP_MasterHoldable_C"}));
    if r4.ok {
        let arr4 = r4.result["instances"].as_array().cloned().unwrap_or_default();
        let live: Vec<_> = arr4.iter()
            .filter(|i| i["is_cdo"].as_bool() != Some(true) && i["full_name"].as_str().unwrap_or("").contains("PersistentLevel"))
            .collect();
        println!("{} live instance(s)", live.len());
        for inst in live.iter().take(3) {
            let name = inst["full_name"].as_str().unwrap_or("?");
            let Some(sel) = selector_of(inst) else { continue };
            let use_hold = read_bytes(&api, &sel, USE_HOLDABLE_SPEEDS, 1).map(|b| b[0]);
            println!("  {name}");
            println!("    UseHoldableMovementSpeeds +0x358 = {use_hold:?}");
        }
    }
}
