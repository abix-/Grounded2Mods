// Per-skill test: armor. Spending raises HC.BaseDamageReduction.

mod common;

use ueforge::client::scenario;

#[test]
fn armor_raises_base_damage_reduction() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "armor")
        .reads(|s: &common::Snapshot| {
            Some(s.live_player.as_ref()?.hc.as_ref()?.base_damage_reduction)
        })
        .should_grow_when_spent();
}
