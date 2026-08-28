//! MISERY connection to Ueforge player input.

pub fn register() {
    ueforge::input::register("misery", &crate::speed::PLAYER);
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
