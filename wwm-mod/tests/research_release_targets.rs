//! Does the 1.0 release still have the classes, fields and
//! methods wwm-mod targets?
//!
//! The game shipped on 2026-08-13 with a new Assembly-CSharp
//! (1,036,800 bytes vs the First Gun demo's 978,432). The demo
//! to First Gun jump already deleted three of the six skill
//! targets (docs/wild-west-miner-research.md section 11), so
//! every target is re-checked from scratch here.
//!
//! Probes:
//! a) control plane answers, op set intact
//! b) every declared Effect target class + field (skills.rs)
//! c) the save-slot key the tracker polls
//! d) the two Harmony postfix target classes
//! e) SkillsManager, the native skill system we want to proxy
//! f) the managers First Gun introduced, to see which survived
//!
//! ```text
//! cargo test -p wwm-mod --test research_release_targets -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{
    api, field_exists, first_handle, is_type_not_found, ping_or_skip, print_declared_methods,
    try_walk, walk,
};
use serde_json::json;

/// Every (class, field) pair skills.rs declares, in catalog order,
/// plus the tracker's save-slot key.
const DECLARED_TARGETS: &[(&str, &str, &str)] = &[
    ("strong_back", "PlayerCarryingController", "_maxCapacity"),
    ("greedy_miner", "MineDataSO", "_oreValue"),
    ("quick_pickaxe", "DigManager", "_digRange"),
    ("charisma", "WorkersManager", "_hireCostMultiplier"),
    ("resilient", "PlayerStaminaController", "_staminaDrainMultiplier"),
    ("slot_key", "GameSerializationSystem", "_currentLoadedSaveNumber"),
];

/// Harmony postfix targets from skills.rs::install_hooks.
const HOOK_TARGETS: &[(&str, &str)] =
    &[("DigManager", "Dig"), ("PlayerManager", "AddPlayerCurrency")];

/// Managers First Gun introduced or kept (research doc 11.3).
const MANAGERS: &[&str] = &[
    "SkillsManager",
    "WeaponsManager",
    "HorsesManager",
    "WildAnimalsManager",
    "HealthStaminaManager",
    "EnergyManager",
    "MissionsManager",
    "PrestigeManager",
    "VillagersManager",
    "WagonsManager",
    "OrdersManager",
    "BuildingsManager",
    "StoreManager",
    "UpgradeManager",
    "BagDatabase",
    "PlayerManager",
];

#[test]
fn control_plane_answers() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let ops = api.op("list_ops", json!({}));
    assert!(ops.ok, "list_ops failed: {:?}", ops.error);
    println!("ops: {}", ops.result);

    // Control: a class that cannot exist must fail, otherwise
    // "walk_class ok" says nothing about the type existing and
    // every survival verdict below is meaningless.
    let bogus = api.op("walk_class", json!({"class": "NoSuchClassZzz"}));
    println!("bogus walk_class ok={} err={:?}", bogus.ok, bogus.error);
    assert!(!bogus.ok, "walk_class accepts unknown classes; survival checks are unreliable");
}

#[test]
fn declared_effect_targets() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    println!("=== declared Effect targets ===");
    let mut alive = 0;
    for (skill, class, field) in DECLARED_TARGETS {
        println!("{skill}:");
        if let Err(e) = try_walk(&api, class) {
            let verdict = if is_type_not_found(&e) { "CLASS GONE" } else { "WALK FAILED" };
            println!("  {class}: {verdict} ({e})");
            continue;
        }
        if field_exists(&api, class, field) {
            alive += 1;
        }
    }
    println!("{alive}/{} declared targets readable", DECLARED_TARGETS.len());
}

#[test]
fn harmony_hook_targets() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    println!("=== Harmony postfix targets ===");
    for (class, method) in HOOK_TARGETS {
        match try_walk(&api, class) {
            Ok(instances) => {
                println!("{class}: resolves, {} live instance(s)", instances.len());
                let methods = api.op("list_methods", json!({"class": class}));
                if methods.ok {
                    // Exact name match. A substring test says "Dig
                    // is present" when the class only has CanDig,
                    // HandleDig and DigDynamite, which is how the
                    // dead DigManager.Dig hook looked alive.
                    let empty = vec![];
                    let listed = methods.result["methods"]
                        .as_array()
                        .unwrap_or(&empty)
                        .iter()
                        .any(|m| m["name"].as_str() == Some(method));
                    println!("  {method}: {}", if listed { "present" } else { "NOT LISTED" });
                } else {
                    println!("  list_methods unavailable: {:?}", methods.error);
                }
            }
            Err(e) if is_type_not_found(&e) => {
                println!("{class}: CLASS GONE (hook {method} will fail)")
            }
            Err(e) => println!("{class}: WALK FAILED ({e})"),
        }
    }
}

/// walk_class returns an empty instance array both when the type
/// is not a UnityEngine.Object and when nothing is in the scene.
/// This separates the two: it prints the resolved FullName (so we
/// see the release's namespace) and asks list_singletons, which
/// goes through Singleton<T>.Instance instead of the scene graph.
#[test]
fn resolve_and_singleton() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    println!("=== raw walk_class (namespace + instances) ===");
    for class in ["DigManager", "PlayerManager", "SkillsManager", "GameSerializationSystem"] {
        let r = api.op("walk_class", json!({"class": class, "include_inactive": true}));
        println!("{class}: ok={} result={}", r.ok, r.result);
    }

    println!("=== list_singletons ===");
    let r = api.op("list_singletons", json!({"types": MANAGERS}));
    println!("ok={} result={}", r.ok, r.result);
}

#[test]
fn native_skills_manager() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    println!("=== SkillsManager ===");
    let Some(handle) = first_handle(&api, "SkillsManager") else {
        println!("SkillsManager: no live instance at this scene");
        return;
    };
    let fields = api.op("inspect_object", json!({"handle": handle}));
    println!("fields: {}", fields.result);
    println!("declared methods:");
    print_declared_methods(&api, "SkillsManager");
}

#[test]
fn manager_survey() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    println!("=== manager survey ===");
    for class in MANAGERS {
        match try_walk(&api, class) {
            Ok(instances) => println!("{class:<24} {} live", instances.len()),
            Err(e) if is_type_not_found(&e) => println!("{class:<24} GONE"),
            Err(e) => println!("{class:<24} WALK FAILED ({e})"),
        }
    }
}
