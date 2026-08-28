// Per-skill test: max_health. Spending raises HC.MaxHealth
// additively (vanilla + max_bonus * progress); refund reverts.

mod common;

use ueforge::client::scenario;

fn max_health(s: &common::Snapshot) -> Option<f32> {
    Some(s.live_player.as_ref()?.hc.as_ref()?.max_health)
}

#[test]
fn max_health_increases() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "max_health")
        .reads(max_health)
        .should_grow_when_spent();
}

#[test]
fn max_health_reverts_on_refund() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "max_health")
        .reads(max_health)
        .should_revert_when_refunded();
}
