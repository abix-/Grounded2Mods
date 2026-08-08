//! Research: the aggro/behaviour surface. How a goon is tasked to
//! fight so spawned mobs stay and attack instead of walking to an
//! exit building (docs/research.md, combat/aggro).
//!
//! ```text
//! cargo test -p schedule1-mod --test research_combat. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, ping_or_skip, print_declared_methods};

#[test]
fn combat_behaviour_surface() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // CartelGoon needs the enlarged list_methods buffer (gen1 hot
    // reload); its answering also proves the reload took.
    for class in [
        "Il2CppScheduleOne.Cartel.CartelGoon",
        "Il2CppScheduleOne.Combat.CombatBehaviour",
        "Il2CppScheduleOne.NPCs.Behaviour.NPCBehaviour",
    ] {
        print_declared_methods(&api, class);
    }
}
