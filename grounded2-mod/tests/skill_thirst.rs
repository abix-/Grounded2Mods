// Per-skill test: thirst. Same shape as hunger, on the thirst
// drain rate.

mod common;

use ueforge::client::scenario;

fn thirst_drain(s: &common::Snapshot) -> Option<f32> {
    Some(s.cdo.survival.as_ref()?.thirst_adjustment_per_second)
}

#[test]
fn thirst_drain_grows_toward_zero_on_spend() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "thirst")
        .reads(thirst_drain)
        .should_grow_when_spent();
}

#[test]
fn thirst_reverts_on_refund() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "thirst")
        .reads(thirst_drain)
        .should_revert_when_refunded();
}
