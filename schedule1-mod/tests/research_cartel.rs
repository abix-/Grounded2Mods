//! Research: the mob-farming and loot machinery (docs/research.md
//! questions 2 and the loot path from the goal checklist).
//!
//! Metadata scan 2026-08-07 found:
//! - SpawnGoon(Vector3) -> CartelGoon and
//!   SpawnMultipleGoons(Vector3, int, bool) -> List<CartelGoon>
//!   as PUBLIC methods; a GoonPool with spawnedGoons /
//!   unspawnedGoons / ReturnToPool; SpawnAmbush(Player, Vector3[]).
//! - Loot candidates: ScheduleOne.ItemFramework.ItemPickup,
//!   NetworkedItemPickup, ScheduleOne.Economy.DeadDrop,
//!   ScheduleOne.ObjectScripts.Cash.
//!
//! This test proves the classes live and maps their declared
//! surfaces. It spawns NOTHING: mutation waits until the operator
//! wants a spawn observed in-game.
//!
//! ```text
//! cargo test -p schedule1-mod --test research_cartel. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, ping_or_skip, print_declared_methods, walk};

#[test]
fn goon_and_loot_machinery() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // Live census of the mob + loot classes.
    for class in [
        "ScheduleOne.Cartel.CartelGoon",
        "ScheduleOne.Cartel.GoonPool",
        "ScheduleOne.Cartel.Ambush",
        "ScheduleOne.Cartel.CartelActivities",
        "ScheduleOne.ItemFramework.ItemPickup",
        "ScheduleOne.Economy.DeadDrop",
    ] {
        walk(&api, class);
    }

    // Declared surfaces: who owns SpawnGoon, what a goon can do,
    // and how pickups are created.
    for class in [
        "Il2CppScheduleOne.Cartel.CartelGoon",
        "Il2CppScheduleOne.Cartel.GoonPool",
        "Il2CppScheduleOne.Cartel.Cartel",
        "Il2CppScheduleOne.ItemFramework.ItemPickup",
        "Il2CppScheduleOne.Economy.DeadDrop",
    ] {
        print_declared_methods(&api, class);
    }
}
