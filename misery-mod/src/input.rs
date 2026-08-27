//! MISERY bindings for Ueforge's player input surface.
//!
//! The engine adapter owns movement, look, batching, and game-thread entry.
//! This module owns the one game-specific fact: the player's `E` action is
//! the generated Enhanced Input interaction handler on its Blueprint class.

use modforge::input::Key;
use ueforge::ue::UObject;

const PLAYER_CLASS: &str = "BP_SGKMasterCharacter_C";
const INTERACT_FUNCTION: &str = "InpActEvt_InteractInput_K2Node_EnhancedInputActionEvent_0";

fn player_action(key: Key, down: bool) -> Result<Option<&'static str>, String> {
    match (key.0, down) {
        (0x45, true) => Ok(Some("interact")),
        (0x45, false) => Ok(None),
        (key, _) => Err(format!(
            "MISERY player input does not bind virtual key 0x{key:02X}"
        )),
    }
}

fn key_input(player: &UObject, key: Key, down: bool) -> Result<(), String> {
    match player_action(key, down)? {
        Some("interact") => {
            unsafe {
                ueforge::ue::pe_call::call_ufunction_zeroed(
                    player,
                    PLAYER_CLASS,
                    INTERACT_FUNCTION,
                )?;
            }
            Ok(())
        }
        Some(action) => Err(format!("MISERY player input has no handler for {action}")),
        None => Ok(()),
    }
}

/// Register MISERY's retained player and game-specific actions with Ueforge.
pub fn register() {
    ueforge::input::register("misery", &crate::speed::PLAYER, key_input);
}

#[cfg(test)]
mod tests {
    use super::player_action;
    use modforge::input::Key;

    #[test]
    fn e_down_is_interact_and_key_up_is_only_release() {
        assert_eq!(player_action(Key(0x45), true).unwrap(), Some("interact"));
        assert_eq!(player_action(Key(0x45), false).unwrap(), None);
        assert!(player_action(Key(0x46), true).is_err());
    }
}
