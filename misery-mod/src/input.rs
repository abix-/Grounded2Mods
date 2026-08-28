//! MISERY connection to Ueforge player input.

use modforge::input::PlayerCommand;

fn player_input(_commands: &[PlayerCommand]) -> Result<(), String> {
    Err("MISERY virtual player input is not connected to Unreal player input yet".into())
}

pub fn register() {
    ueforge::input::register("misery", &crate::speed::PLAYER, player_input);
}

#[cfg(test)]
mod tests {
    #[test]
    fn misery_input_never_calls_a_gameplay_action_directly() {
        let source = include_str!("input.rs");
        for forbidden in [
            "call_ufunction",
            "process_event",
            "AddMovement",
            "AddYaw",
            "AddPitch",
        ] {
            assert!(!source.contains(forbidden));
        }
    }
}
