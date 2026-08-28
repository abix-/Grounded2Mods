// Per-skill test: attack_damage. Spending raises ASC's
// CustomDamageMultiplier on the live pawn.

mod common;

use ueforge::client::scenario;

#[test]
fn attack_damage_multiplier_grows_with_spend() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "attack_damage")
        .reads(|s: &common::Snapshot| {
            Some(
                s.live_player
                    .as_ref()?
                    .asc
                    .as_ref()?
                    .custom_damage_multiplier,
            )
        })
        .should_grow_when_spent();
}
