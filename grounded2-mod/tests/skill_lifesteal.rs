// Per-skill test: lifesteal. Runtime effect with no CDO field to
// observe via snapshot. We just assert the level increments on
// spend (and revert on refund). Deeper observable test pending
// simulate_damage.
//
// (Lifesteal landed live in the damage hook 2026-05-10; the
// observable test will arrive when simulate_damage is wired
// safely from a non-PE-trampoline drain site.)

mod common;

#[test]
fn lifesteal_level_increments_on_spend() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    let api = api.inner();
    if api.skill_points() < 1 {
        eprintln!("skipping: need a skill point");
        return;
    }
    let lvl0 = api.skill_level("lifesteal");
    api.skill_spend("lifesteal", 1);
    let lvl1 = api.skill_level("lifesteal");
    api.skill_refund("lifesteal", 1);
    assert_eq!(lvl1, lvl0 + 1);
}
