//! Spawn one NPC on the game thread (research.md 26, spawning
//! groundwork for the NPC multiplier).
//!
//! Calls the engine's AIBlueprintHelperLibrary:SpawnAIFromClass
//! (the same function the game's own spawn point uses) through
//! the `call` op, which runs it on the game thread via the PE
//! drain. Parm layout from the object dump:
//!
//! | 0x00 | WorldContextObject | UObject*  |
//! | 0x08 | PawnClass          | UClass*   |
//! | 0x10 | BehaviorTree       | UObject*  |
//! | 0x18 | Location           | 3 doubles |
//! | 0x30 | Rotation           | 3 doubles |
//! | 0x48 | bNoCollisionFail   | bool      |
//! | 0x50 | Owner              | AActor*   |
//! | 0x58 | ReturnValue        | APawn*    |
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_spawn -- --ignored --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use modforge::client;

type Api = common::Api;

const NPC_CHAIN: &str = "BP_MasterAICharacter_C";
const PLAYER_CHAIN: &str = "BP_SGKMasterCharacter_C";

fn player_location(api: &Api, player_sel: &str) -> Option<(f64, f64, f64)> {
    // K2_GetActorLocation: ReturnValue FVector at offset 0.
    let parms = vec![0u8; 0x18];
    let (out, _) = api
        .call_ufunction("Actor", "K2_GetActorLocation", player_sel, &parms)
        .ok()?;
    if out.len() < 0x18 {
        return None;
    }
    Some((
        client::from_le_f64(&out, 0x00),
        client::from_le_f64(&out, 0x08),
        client::from_le_f64(&out, 0x10),
    ))
}

/// Spawn one copy of the first live hostile NPC's class next to
/// the player and assert the census grows by one.
#[test]
#[ignore = "writes to live game"]
fn spawn_one_npc() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }

    let npcs = client::walk_class_chain_instances(&api, NPC_CHAIN, 400);
    let count_before = npcs.len();
    let Some(donor) = npcs.iter().find(|n| !n.name.contains("Tamed")) else {
        println!("SKIP: no live hostile NPC to copy (go to an expedition)");
        return;
    };
    // Donor's class pointer: UObject::ClassPrivate at +0x10.
    let class_b = client::read_bytes(&api, donor.addr, 0x10, 8);
    assert_eq!(class_b.len(), 8, "could not read donor class ptr");
    let donor_class = client::from_le_u64(&class_b, 0);
    println!("donor {} class {donor_class:#x}, census before: {count_before}", donor.name);

    let players = client::walk_class_chain_instances(&api, PLAYER_CHAIN, 4);
    let Some(player) = players.first() else {
        println!("SKIP: no live player");
        return;
    };
    let Some((x, y, z)) = player_location(&api, &player.addr_selector) else {
        panic!("K2_GetActorLocation failed");
    };
    println!("player at ({x:.0}, {y:.0}, {z:.0})");

    // Library CDO to invoke the static function on.
    let lib = client::find_class_cdo(&api, "AIBlueprintHelperLibrary")
        .expect("AIBlueprintHelperLibrary CDO not found");

    let mut parms = vec![0u8; 0x60];
    parms[0x00..0x08].copy_from_slice(&player.addr.to_le_bytes());
    parms[0x08..0x10].copy_from_slice(&donor_class.to_le_bytes());
    // BehaviorTree stays null; SmartAI NPCs drive themselves.
    parms[0x18..0x20].copy_from_slice(&(x + 300.0).to_le_bytes());
    parms[0x20..0x28].copy_from_slice(&(y + 300.0).to_le_bytes());
    parms[0x28..0x30].copy_from_slice(&(z + 100.0).to_le_bytes());
    parms[0x48] = 1; // bNoCollisionFail: always spawn
    let (out, _) = api
        .call_ufunction(
            "AIBlueprintHelperLibrary",
            "SpawnAIFromClass",
            &lib.addr_selector,
            &parms,
        )
        .expect("SpawnAIFromClass call failed");
    assert!(out.len() >= 0x60, "short parms returned: {}", out.len());
    let pawn = client::from_le_u64(&out, 0x58);
    println!("spawned pawn = {pawn:#x}");
    assert_ne!(pawn, 0, "SpawnAIFromClass returned null pawn");

    let after = client::walk_class_chain_instances(&api, NPC_CHAIN, 400);
    println!("census after: {}", after.len());
    assert_eq!(after.len(), count_before + 1, "census did not grow by one");
}
