//! Research for the combat-XP levelling slice: the concrete
//! anchor points the skill catalog needs.
//!
//! 1. PlayerHealth (Il2CppScheduleOne.PlayerScripts.Health.
//!    PlayerHealth): the max-health field name and whether the
//!    class is singleton-reachable (UnityField effects resolve
//!    singletons; a component needs a custom effect).
//! 2. PunchController (ScheduleOne.Combat): the damage field
//!    for a melee-damage skill.
//! 3. Save-slot identity: what the persistence layer exposes for
//!    "which save is loaded" (the Tracker's per-slot store key).
//!
//! ```text
//! cargo test -p schedule1-mod --test research_levelling. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, first_handle, ping_or_skip, print_declared_methods};
use serde_json::json;

#[test]
fn levelling_anchor_points() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    for class in [
        "Il2CppScheduleOne.PlayerScripts.Health.PlayerHealth",
        "Il2CppScheduleOne.Combat.PunchController",
        "Il2CppScheduleOne.Persistence.LoadManager",
        "Il2CppScheduleOne.Persistence.SaveManager",
    ] {
        print_declared_methods(&api, class);
    }

    // Singleton reachability: which of these answer Instance?
    let singles = api.op(
        "list_singletons",
        json!({"types": [
            "Il2CppScheduleOne.PlayerScripts.Health.PlayerHealth",
            "Il2CppScheduleOne.PlayerScripts.Player",
            "Il2CppScheduleOne.Combat.PunchController",
            "Il2CppScheduleOne.Persistence.LoadManager",
            "Il2CppScheduleOne.Persistence.SaveManager",
        ]}),
    );
    println!("singletons: {}", singles.result);

    // Live PlayerHealth fields (the max-health name).
    if let Some(h) = first_handle(&api, "ScheduleOne.PlayerScripts.Health.PlayerHealth") {
        let inspect = api.op("inspect_object", json!({"handle": h}));
        println!(
            "PlayerHealth[0] inspect:\n{}",
            serde_json::to_string_pretty(&inspect.result).unwrap_or_default()
        );
    }
}
