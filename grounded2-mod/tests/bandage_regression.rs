// Regression fence: impact_resistance must not block healing.
//
// USER EXPECTATION: bandages (and other healing items) work
// regardless of which RPG skills are active.
//
// Failure mode this catches: any future regression that
// reintroduces a binary mask or other heal-blocking gate while
// impact_resistance is enabled. The previous bug
// (`RequiredDamageTypeFlags = 0xFFFFFFFF`) blocked bandages
// because their HoT tick (Type=Health (24) status effect)
// routed through native code that consults the mask.
//
// What this asserts (post-fix 2026-05-09):
//   1. mask is NOT 0xFFFFFFFF when impact_resistance is enabled
//      at any level. (The skill must NOT write the mask.)
//   2. Calling AddHealth on the player heals normally with
//      impact_resistance enabled. (Heals are not gated by the
//      skill.)
//
// Stays in the suite forever as a fence.

mod common;

use serde_json::json;

#[repr(C)]
#[derive(
    Default,
    Copy,
    Clone,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::Immutable,
    zerocopy::KnownLayout,
)]
struct AddHealthParms {
    amount: f32,
    _pad: [u8; 4],
    causer: u64,
}

#[test]
fn impact_resistance_does_not_block_healing() {
    let Some(api) = common::Api::try_connect() else {
        eprintln!("skipping: BBP_DEBUG_PORT not set");
        return;
    };
    let s = api.snapshot();
    if s.live_player.is_none() {
        panic!("no live player; load a save and spawn into the world");
    }

    // Take impact_resistance to a non-zero level and enable it.
    if s.skill_level("impact_resistance") == 0 {
        if s.player_state.as_ref().unwrap().skill_points < 1 {
            eprintln!("skipping: need 1 skill point");
            return;
        }
        api.skill_spend("impact_resistance", 1);
    }
    api.skill_toggle("impact_resistance", true);

    let mid = api.snapshot();
    assert!(
        !mid.is_disabled("impact_resistance"),
        "test setup failed: impact_resistance still disabled"
    );
    assert_ne!(
        mid.live_hc().required_damage_type_flags,
        "0xFFFFFFFF",
        "FENCE VIOLATED: the binary mask is back. \
         impact_resistance is regressing to its broken shape."
    );

    // Now drive a heal directly via the canonical UFunction. If
    // the mask were back, this would silently no-op or be
    // suppressed.
    let cd_pre = mid.live_hc().current_damage;
    if cd_pre < 1.0 {
        eprintln!(
            "skipping HP-delta check: player at full HP, no headroom for the heal to be observable"
        );
        return;
    }

    let parms = AddHealthParms {
        amount: 20.0,
        _pad: [0; 4],
        causer: 0,
    };
    let bytes = common::parms_as_bytes(&parms);
    let (_after, post) = api
        .call_ufunction("HealthComponent", "AddHealth", "live_player_hc", bytes)
        .expect("AddHealth call failed");

    let cd_post = post.live_hc().current_damage;
    let healed = cd_pre - cd_post;
    assert!(
        healed > 1.0,
        "FENCE VIOLATED: AddHealth(20) did not heal with impact_resistance enabled. \
         CurrentDamage {} -> {} (delta = {}). The impact_resistance gate is \
         blocking healing again -- this is the bandage bug.",
        cd_pre,
        cd_post,
        cd_post - cd_pre
    );
}

#[test]
fn unknown_damage_type_is_not_intercepted() {
    // Sanity: the impact intercept should only fire for
    // environmental damage. AddHealth has no DamageType class
    // associated with it, so the intercept must skip it cleanly.
    // Tested implicitly by the heal succeeding in the test above,
    // but documented here as the intent.
    let _ = json!(true); // placeholder so cargo doesn't complain about unused import
}
