#[test]
fn production_bot_input_has_no_player_route_bypasses() {
    let sources = [
        ("ueforge input", include_str!("../../ueforge/src/input.rs")),
        ("misery input", include_str!("../src/input.rs")),
    ];
    for (name, source) in sources {
        for forbidden in [
            "Input.+key",
            "Input.-key",
            "KismetSystemLibrary.ExecuteConsoleCommand",
            "SimpleMoveToLocation",
            "AddMovementInput",
            "AddYawInput",
            "AddPitchInput",
            "Input_K2Node_EnhancedInputActionEvent",
            "K2_SetActorLocation",
            "K2_SetActorRotation",
            "SetActorTransform",
        ] {
            assert!(
                !source.contains(forbidden),
                "{name} bypasses player input with {forbidden}"
            );
        }
    }
}
