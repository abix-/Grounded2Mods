// Per-skill test: hunger. Spending reduces drain rate; the rate
// is negative, so "less drain" = greater (closer to zero) value.

mod common;

use ueforge::client::scenario;

fn hunger_drain(s: &common::Snapshot) -> Option<f32> {
    Some(s.cdo.survival.as_ref()?.hunger_adjustment_per_second)
}

#[test]
fn hunger_drain_grows_toward_zero_on_spend() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    // drain is negative; less drain = closer to 0 = strictly greater.
    scenario::for_skill(api.inner(), "hunger")
        .reads(hunger_drain)
        .should_grow_when_spent();
}

#[test]
fn hunger_reverts_on_refund() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "hunger")
        .reads(hunger_drain)
        .should_revert_when_refunded();
}
