// Per-skill test: jump_height. Spending raises CMC.JumpZVelocity.

mod common;

use ueforge::client::scenario;

#[test]
fn jump_height_grows_with_spend() {
    let Some(api) = common::Api::try_connect() else {
        return;
    };
    scenario::for_skill(api.inner(), "jump_height")
        .reads(|s: &common::Snapshot| Some(s.live_player.as_ref()?.cmc.as_ref()?.jump_z_velocity))
        .should_grow_when_spent();
}
