//! Does `SkillsManager.SetSkillLevel` work on the 1.0 build?
//!
//! The whole proxy-Effect plan (research doc section 13/14)
//! rests on this one call: if the mod can set a vanilla skill
//! level, the game does the stat change, the UI refresh and the
//! save, and wwm-mod never writes a game field again.
//!
//! This test WRITES to the loaded save. It picks Bag because
//! `Bag.GetSlotsAmount()` recomputes from the skill on every
//! call (section 14.3), so the effect is observable immediately
//! and needs no reload. It restores the original level before
//! asserting anything, so a failed assert cannot leave the save
//! modified.
//!
//! ```text
//! cargo test -p wwm-mod --test research_skill_write -- --nocapture
//! ```

mod common;
use common::{api, first_handle, ping_or_skip};
use modforge::client::Api;
use serde_json::{Value, json};

/// The level to write. Bag has 5 levels; 3 is mid-range, so the
/// test never lands on the vanilla value or the ceiling.
const PROBE_LEVEL: i64 = 3;

/// Bag catalog rows from section 13.2: value is the slot count.
const BAG_VALUE_AT_LEVEL: [f64; 5] = [5.0, 7.0, 9.0, 12.0, 15.0];

fn skill_level(api: &Api<Value>, mgr: i64, skill: &str) -> Option<i64> {
    api.op(
        "invoke_method",
        json!({"handle": mgr, "method": "GetCurrentSkillLevel", "args": [skill]}),
    )
    .result
    .as_i64()
}

fn skill_value(api: &Api<Value>, mgr: i64, skill: &str) -> Option<f64> {
    api.op(
        "invoke_method",
        json!({"handle": mgr, "method": "GetCurrentSkillValue", "args": [skill, false]}),
    )
    .result
    .as_f64()
}

/// Bag.GetSlotsAmount(), the consumer that proves the game acted
/// on the write rather than just storing a number.
fn bag_slots(api: &Api<Value>) -> Option<i64> {
    let bag = first_handle(api, "Bag")?;
    api.op(
        "invoke_method",
        json!({"handle": bag, "method": "GetSlotsAmount", "args": []}),
    )
    .result
    .as_i64()
}

/// Set one vanilla skill to a level and LEAVE IT THERE. Unlike
/// every other test here this one deliberately changes the
/// player's save, so it is #[ignore]d and takes its target from
/// the environment rather than hardcoding one:
///
/// ```text
/// WWM_SKILL=Speed WWM_LEVEL=max cargo test -p wwm-mod \
///   --test research_skill_write set_skill -- --ignored --nocapture
/// ```
///
/// WWM_SKILL is Bag | Energy | Rope | Speed. WWM_LEVEL is a
/// number or `max`. The level is clamped to the skill's own
/// ceiling: nothing in the game clamps it, and a level past the
/// end throws the next time anything reads the value.
#[test]
#[ignore = "writes the player's save on purpose"]
fn set_skill() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let skill = std::env::var("WWM_SKILL").expect("set WWM_SKILL (Bag|Energy|Rope|Speed)");
    let want = std::env::var("WWM_LEVEL").expect("set WWM_LEVEL (a number or 'max')");

    let Some(mgr) = first_handle(&api, "SkillsManager") else {
        println!("SkillsManager: no live instance; load a save first");
        return;
    };

    let levels = api
        .op(
            "invoke_method",
            json!({"handle": mgr, "method": "GetSkillLevelsAmount", "args": [skill]}),
        )
        .result
        .as_i64()
        .unwrap_or_else(|| panic!("could not read the {skill} level count"));

    let target = if want == "max" {
        levels
    } else {
        want.parse::<i64>()
            .expect("WWM_LEVEL must be a number or 'max'")
            .clamp(1, levels)
    };

    let before = skill_level(&api, mgr, &skill);
    let before_value = skill_value(&api, mgr, &skill);
    println!("before: {skill} level={before:?}/{levels} value={before_value:?}");

    let write = api.op(
        "invoke_method",
        json!({"handle": mgr, "method": "SetSkillLevel", "args": [skill, target]}),
    );
    assert!(write.ok, "SetSkillLevel failed: {:?}", write.error);

    let after = skill_level(&api, mgr, &skill);
    let after_value = skill_value(&api, mgr, &skill);
    let second = api.op(
        "invoke_method",
        json!({"handle": mgr, "method": "GetCurrentSkillValue", "args": [skill, true]}),
    );
    println!(
        "after:  {skill} level={after:?}/{levels} value={after_value:?} value2={}",
        second.result
    );

    assert_eq!(after, Some(target), "{skill} did not reach level {target}");
}

#[test]
fn set_skill_level_moves_bag_capacity() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let Some(mgr) = first_handle(&api, "SkillsManager") else {
        println!("SkillsManager: no live instance; load a save first");
        return;
    };

    let original = skill_level(&api, mgr, "Bag").expect("could not read the Bag level");
    let before_value = skill_value(&api, mgr, "Bag");
    let before_slots = bag_slots(&api);
    println!("before: level={original} value={before_value:?} slots={before_slots:?}");
    assert!(
        original != PROBE_LEVEL,
        "save already sits at the probe level {PROBE_LEVEL}; pick another"
    );

    let write = api.op(
        "invoke_method",
        json!({"handle": mgr, "method": "SetSkillLevel", "args": ["Bag", PROBE_LEVEL]}),
    );
    println!(
        "SetSkillLevel(Bag, {PROBE_LEVEL}): ok={} {:?}",
        write.ok, write.error
    );

    let after_level = skill_level(&api, mgr, "Bag");
    let after_value = skill_value(&api, mgr, "Bag");
    let after_slots = bag_slots(&api);
    println!("after:  level={after_level:?} value={after_value:?} slots={after_slots:?}");

    // Put the save back before any assert can abort the test.
    let restore = api.op(
        "invoke_method",
        json!({"handle": mgr, "method": "SetSkillLevel", "args": ["Bag", original]}),
    );
    let restored_level = skill_level(&api, mgr, "Bag");
    let restored_slots = bag_slots(&api);
    println!(
        "restore to {original}: ok={} level={restored_level:?} slots={restored_slots:?}",
        restore.ok
    );

    assert!(write.ok, "SetSkillLevel failed: {:?}", write.error);
    assert_eq!(after_level, Some(PROBE_LEVEL), "the level did not change");
    assert_eq!(
        after_value,
        Some(BAG_VALUE_AT_LEVEL[(PROBE_LEVEL - 1) as usize]),
        "skill value does not match the catalog row for this level"
    );
    assert_eq!(
        after_slots,
        Some(BAG_VALUE_AT_LEVEL[(PROBE_LEVEL - 1) as usize] as i64),
        "Bag.GetSlotsAmount did not follow the skill level"
    );
    assert_eq!(
        restored_level,
        Some(original),
        "SAVE LEFT MODIFIED: restore failed"
    );
}
