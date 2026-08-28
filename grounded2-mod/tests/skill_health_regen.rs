// Per-skill test: health_regen. Spending raises tick percentage
// + lowers tick rate (more frequent ticks) on UGlobalCombatData.
// Toggle off reverts.

mod common;

use ueforge::client::scenario;

fn tick_pct(s: &common::Snapshot) -> Option<f32> {
    Some(
        s.cdo
            .global_combat_data
            .as_ref()?
            .combat_regen_tick_percentage,
    )
}
fn tick_rate(s: &common::Snapshot) -> Option<f32> {
    Some(s.cdo.global_combat_data.as_ref()?.combat_regen_tick_rate)
}

#[test]
fn health_regen_tick_pct_grows() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "health_regen")
        .reads(tick_pct)
        .should_grow_when_spent();
}

#[test]
fn health_regen_tick_rate_shrinks() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "health_regen")
        .reads(tick_rate)
        .should_shrink_when_spent();
}

#[test]
fn health_regen_toggle_reverts() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "health_regen")
        .reads(tick_pct)
        .should_revert_when_toggled_off();
}
