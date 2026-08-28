// Per-skill test: glide_speed. Spending raises CMC.MaxFlySpeed
// and CustomFlySpeedMultiplier.

mod common;

use ueforge::client::scenario;

#[test]
fn glide_speed_grows() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "glide_speed")
        .reads(|s: &common::Snapshot| Some(s.live_player.as_ref()?.cmc.as_ref()?.max_fly_speed))
        .should_grow_when_spent();
}

#[test]
fn glide_speed_custom_multiplier_grows() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "glide_speed")
        .reads(|s: &common::Snapshot| {
            Some(
                s.live_player
                    .as_ref()?
                    .cmc
                    .as_ref()?
                    .custom_fly_speed_multiplier,
            )
        })
        .should_grow_when_spent();
}
