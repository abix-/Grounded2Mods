// Failing test: skill_refund op.
//
// Refunding should decrement skill_levels[id] (or remove the entry
// at 0) and increment skill_points. Refund below 0 should clamp.
//
// Red until `skill_refund` op exists; goes green when implemented.

mod common;

#[test]
fn refund_reverses_a_spend() {
    let Some(api) = common::Api::try_connect() else {
        eprintln!("skipping: BBP_DEBUG_PORT not set");
        return;
    };

    let before = api.snapshot();
    let Some(ps_before) = before.player_state.as_ref() else {
        panic!("no slot loaded");
    };
    if ps_before.skill_points < 1 {
        panic!("test needs at least 1 skill point");
    }

    let lvl_before = before.skill_level("backpack");
    api.skill_spend("backpack", 1);
    let after_spend = api.skill_refund("backpack", 1);

    assert_eq!(
        after_spend.skill_level("backpack"),
        lvl_before,
        "refund did not revert the spend"
    );
    assert_eq!(
        after_spend
            .player_state
            .as_ref()
            .expect("player_state")
            .skill_points,
        ps_before.skill_points,
        "skill_points not back to original"
    );
}

#[test]
fn refund_below_zero_clamps() {
    let Some(api) = common::Api::try_connect() else {
        eprintln!("skipping: BBP_DEBUG_PORT not set");
        return;
    };

    // Pick a skill we can put at level 0 reliably.
    let snap = api.snapshot();
    let id = "lifesteal";
    let lvl = snap.skill_level(id);
    if lvl > 0 {
        api.skill_refund(id, lvl);
    }

    // Refund 5 from a skill at level 0 should not error and not
    // change the level.
    let r = api.op("skill_refund", serde_json::json!({"id": id, "count": 5}));
    assert!(r.ok, "refund-from-0 should not error: {:?}", r.error);
    assert_eq!(r.state.skill_level(id), 0);
}
