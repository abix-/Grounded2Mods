// Per-skill test: leap_distance. Spending raises three CMC fields
// (AirControl, BoostMultiplier, BoostVelocityThreshold).

mod common;

use ueforge::client::scenario;

#[test]
fn leap_distance_air_control_grows() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "leap_distance")
        .reads(|s: &common::Snapshot| Some(s.live_player.as_ref()?.cmc.as_ref()?.air_control))
        .should_grow_when_spent();
}

#[test]
fn leap_distance_boost_multiplier_grows() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "leap_distance")
        .reads(|s: &common::Snapshot| {
            Some(
                s.live_player
                    .as_ref()?
                    .cmc
                    .as_ref()?
                    .air_control_boost_multiplier,
            )
        })
        .should_grow_when_spent();
}

#[test]
fn leap_distance_boost_threshold_grows() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "leap_distance")
        .reads(|s: &common::Snapshot| {
            Some(
                s.live_player
                    .as_ref()?
                    .cmc
                    .as_ref()?
                    .air_control_boost_velocity_threshold,
            )
        })
        .should_grow_when_spent();
}
