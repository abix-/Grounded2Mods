// Per-skill test: move_speed. Spending raises MaxWalkSpeed;
// toggling off reverts to vanilla; toggling on restores.

mod common;

use ueforge::client::scenario;

fn walk_speed(s: &common::Snapshot) -> Option<f32> {
    Some(s.live_player.as_ref()?.cmc.as_ref()?.max_walk_speed)
}

#[test]
fn move_speed_grows() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "move_speed")
        .reads(walk_speed)
        .should_grow_when_spent();
}

#[test]
fn move_speed_toggle_reverts() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "move_speed")
        .reads(walk_speed)
        .should_revert_when_toggled_off();
}
