//! Research questions 2 + 3 (docs/research.md): NPC classes,
//! combat/death path, and the kill-observation hook point.
//!
//! Metadata scan 2026-08-07 found (see certainty-tracking.md):
//! - ScheduleOne.NPCs: NPC, NPCHealth, NPCManager, NPCMovement,
//!   NPCInventory, Behaviour.*; CharacterClasses.SewerGoblin.
//! - ScheduleOne.Combat: CombatBehaviour, PunchController,
//!   IDamageable.
//! - Death path names: Die, OnDied, KnockOut, DiedOrKnockedOut,
//!   TakeDamage(Single,Boolean,Boolean) as FishNet RPCs.
//!
//! This test proves them live: NPC census, NPCHealth's declared
//! surface, and harmony_probe on the kill-hook candidates (the
//! probe patches with a no-op prefix, reports, and unpatches, so
//! it is safe on a live game).
//!
//! ```text
//! cargo test -p schedule1-mod --test research_npcs. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, first_handle, ping_or_skip, print_declared_methods, walk};
use serde_json::json;

#[test]
fn npc_and_death_path() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // NPC census: how many NPCs are live right now.
    walk(&api, "ScheduleOne.NPCs.NPC");

    // The classes that gate combat XP: what do they declare?
    for class in [
        "Il2CppScheduleOne.NPCs.NPCHealth",
        "Il2CppScheduleOne.NPCs.NPCManager",
        "Il2CppScheduleOne.Combat.CombatBehaviour",
    ] {
        print_declared_methods(&api, class);
    }

    // First NPC: inspect the health link end to end.
    if let Some(h) = first_handle(&api, "ScheduleOne.NPCs.NPCHealth") {
        let inspect = api.op("inspect_object", json!({"handle": h}));
        println!(
            "NPCHealth[0] inspect:\n{}",
            serde_json::to_string_pretty(&inspect.result).unwrap_or_default()
        );
    }

    // Kill-hook candidates: prove per-target patchability without
    // keeping any patch installed.
    for method in ["Die", "OnDied", "KnockOut"] {
        let r = api.op(
            "harmony_probe",
            json!({"class": "Il2CppScheduleOne.NPCs.NPCHealth", "method": method}),
        );
        if r.ok {
            println!("harmony_probe NPCHealth.{method}: {}", r.result);
        } else {
            println!("harmony_probe NPCHealth.{method} FAILED: {:?}", r.error);
        }
    }
}
