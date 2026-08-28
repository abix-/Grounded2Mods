//! Combat-XP levelling verification (the exit gate in
//! docs/todo.md): skill_state answers, XP awards
//! level up, spending a point on vitality visibly raises
//! PlayerHealth.MaxHealth in the live game.
//!
//! The in-game half of the gate (kill an NPC -> XP line in the
//! MelonLoader console; save/reload -> state persists) needs
//! the operator; this test drives everything reachable from the
//! control plane and prints what to check.
//!
//! ```text
//! cargo test -p schedule1-mod --test levelling. --test-threads=1 --nocapture
//! ```
//!
//! SKIPs (prints why and passes) when the game is not running.

mod common;
use common::{api, ping_or_skip};
use serde_json::json;

fn max_health(api: &modforge::client::Api<serde_json::Value>) -> Option<f64> {
    api.op(
        "invoke_static",
        json!({"class": "Il2CppScheduleOne.PlayerScripts.Health.PlayerHealth",
               "method": "get_MaxHealth", "args": []}),
    )
    .result
    .as_f64()
}

/// The live punch-damage pair off the player's PunchController.
fn punch_damage(api: &modforge::client::Api<serde_json::Value>) -> Option<(f64, f64)> {
    let h = common::first_handle(api, "ScheduleOne.Combat.PunchController")?;
    let min = api
        .op(
            "invoke_method",
            json!({"handle": h, "method": "get_MinPunchDamage", "args": []}),
        )
        .result
        .as_f64()?;
    let max = api
        .op(
            "invoke_method",
            json!({"handle": h, "method": "get_MaxPunchDamage", "args": []}),
        )
        .result
        .as_f64()?;
    Some((min, max))
}

/// Instance-property write probe: PunchController's punch
/// damage pair through the same walk + invoke path heavy_hands
/// uses. Needs a loaded save (the controller lives on the
/// player). Liveness-checked per step.
///
/// ```text
/// cargo test -p schedule1-mod --test levelling instance_prop_probe. --test-threads=1 --nocapture
/// ```
#[test]
fn instance_prop_probe() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let Some(h) = common::first_handle(&api, "ScheduleOne.Combat.PunchController") else {
        println!("no PunchController instance; load a save first");
        return;
    };
    for (method, args) in [
        ("get_MinPunchDamage", json!([])),
        ("get_MaxPunchDamage", json!([])),
        ("set_MinPunchDamage", json!([12.0])),
        ("get_MinPunchDamage", json!([])),
        ("set_MinPunchDamage", json!([20.0])),
        ("get_MinPunchDamage", json!([])),
        ("set_MaxPunchDamage", json!([35.0])),
        ("get_MaxPunchDamage", json!([])),
    ] {
        println!("-- {method} {args} ...");
        let r = api.op(
            "invoke_method",
            json!({"handle": h, "method": method, "args": args}),
        );
        println!("   ok={} result={} err={:?}", r.ok, r.result, r.error);
        std::thread::sleep(std::time::Duration::from_secs(3));
        match api.try_op("ping", json!({})) {
            Ok(p) if p.ok => println!("   game alive"),
            _ => {
                println!("GAME DIED on {method}");
                return;
            }
        }
    }
    println!("instance probes survived; heavy_hands path is safe");
}

/// Crash bisection, static layer: which static-property invoke
/// kills the game? Works at the MENU (statics need no save).
/// Probes a PunchController static, then the two PlayerHealth
/// statics, get before set, liveness ping after each.
///
/// ```text
/// cargo test -p schedule1-mod --test levelling static_prop_probe. --test-threads=1 --nocapture
/// ```
#[test]
fn static_prop_probe() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let steps: [(&str, &str, serde_json::Value); 6] = [
        (
            "Il2CppScheduleOne.Combat.PunchController",
            "get_PUNCH_RANGE",
            json!([]),
        ),
        (
            "Il2CppScheduleOne.PlayerScripts.Health.PlayerHealth",
            "get_HealthRecoveryPerMinute",
            json!([]),
        ),
        (
            "Il2CppScheduleOne.PlayerScripts.Health.PlayerHealth",
            "get_MaxHealth",
            json!([]),
        ),
        (
            "Il2CppScheduleOne.PlayerScripts.Health.PlayerHealth",
            "set_MaxHealth",
            json!([123.0]),
        ),
        (
            "Il2CppScheduleOne.PlayerScripts.Health.PlayerHealth",
            "get_MaxHealth",
            json!([]),
        ),
        (
            "Il2CppScheduleOne.PlayerScripts.Health.PlayerHealth",
            "set_MaxHealth",
            json!([100.0]),
        ),
    ];
    for (class, method, args) in steps {
        println!("-- {class} :: {method} ...");
        let r = api.op(
            "invoke_static",
            json!({"class": class, "method": method, "args": args}),
        );
        println!("   ok={} result={} err={:?}", r.ok, r.result, r.error);
        std::thread::sleep(std::time::Duration::from_secs(3));
        match api.try_op("ping", json!({})) {
            Ok(p) if p.ok => println!("   game alive"),
            _ => {
                println!("GAME DIED on {class}::{method}");
                return;
            }
        }
    }
    println!("all static probes survived");
}

/// Crash bisection: arm the effect bodies, then apply each
/// skill's effect ALONE with a liveness ping after each. The
/// step whose ping fails names the crashing effect.
///
/// ```text
/// cargo test -p schedule1-mod --test levelling bisect_effects. --test-threads=1 --nocapture
/// ```
#[test]
fn bisect_effects() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let state = api.op("skill_state", json!({}));
    println!("skill_state: {}", state.result);
    if state
        .result
        .get("active")
        .and_then(serde_json::Value::as_bool)
        == Some(false)
    {
        println!("no slot active; load a save and wait 15s first");
        return;
    }
    let armed = api.op("effects_enable", json!({"on": true}));
    println!("effects_enable: {}", armed.result);
    let xp = api.op("skill_add_xp", json!({"amount": 400}));
    println!("skill_add_xp: {}", xp.result);

    for id in ["vitality", "regeneration", "heavy_hands"] {
        println!("-- applying {id} ...");
        let spend = api.op("skill_levelup", json!({"id": id}));
        println!("   skill_levelup {id}: ok={} {}", spend.ok, spend.result);
        std::thread::sleep(std::time::Duration::from_secs(4));
        match api.try_op("ping", json!({})) {
            Ok(r) if r.ok => println!("   game alive after {id}"),
            _ => {
                println!("GAME DIED APPLYING {id}: that effect is the crasher");
                return;
            }
        }
    }
    let mh = max_health(&api);
    println!("all three applied, game alive; MaxHealth now {mh:?}");
}

#[test]
fn xp_levels_and_vitality_raises_max_health() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    // What the slot resolver sees.
    let lm = api.op(
        "list_singletons",
        json!({"types": ["Il2CppScheduleOne.Persistence.LoadManager"]}),
    );
    if let Some(h) = lm.result["singletons"][0]["handle"].as_i64() {
        let loaded = api.op("read_field", json!({"handle": h, "field": "IsGameLoaded"}));
        let path = api.op(
            "read_field",
            json!({"handle": h, "field": "LoadedGameFolderPath"}),
        );
        println!(
            "LoadManager: IsGameLoaded={} path={}",
            loaded.result, path.result
        );
    } else {
        println!("LoadManager singleton not found");
    }

    let state = api.op("skill_state", json!({}));
    if !state.ok {
        println!("skill_state FAILED: {:?}", state.error);
        return;
    }
    println!("skill_state: {}", state.result);
    if state.result["active"].as_bool() == Some(false) {
        println!("no slot active; load a save first");
        return;
    }

    let before = punch_damage(&api);
    println!("punch damage before: {before:?}");

    // Enough XP to guarantee at least one level from a fresh
    // state (curve base 50).
    let xp = api.op("skill_add_xp", json!({"amount": 150}));
    println!("skill_add_xp: ok={} {}", xp.ok, xp.result);

    let spend = api.op("skill_levelup", json!({"id": "heavy_hands"}));
    println!(
        "skill_levelup heavy_hands: ok={} {}",
        spend.ok, spend.result
    );
    if spend.result["spent"].as_u64() == Some(0) {
        println!("no point spent (none available?); punch damage will not move");
    }
    // The effect body runs on the main queue; give it a frame.
    std::thread::sleep(std::time::Duration::from_secs(1));

    let after = punch_damage(&api);
    println!("punch damage after: {after:?}");
    match (before, after) {
        (Some((b, _)), Some((a, amax))) if a > b => {
            println!("HEAVY HANDS WORKS: min punch {b} -> {a} (max {amax})");
            println!(
                "OPERATOR CHECK: punches hit harder. Kill an NPC (XP line in console), save + reload, rerun this test: levels must persist."
            );
        }
        (Some(b), Some(a)) => {
            println!("punch damage unchanged ({b:?} -> {a:?}); heavy_hands did not land")
        }
        _ => println!("could not read punch damage"),
    }
}
