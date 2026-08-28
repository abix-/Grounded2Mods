//! Research how the live MISERY player receives keyboard input.
//!
//! This test derives the retained player's controller and PlayerInput object
//! from the live object data, then prints the input-related functions and
//! parameter layouts exposed by the exact classes MISERY loaded.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 k3sc cargo-lock test -p misery-mod \
//!   --test research_player_input -- --ignored --test-threads=1 --nocapture
//! ```

mod common;

use common::{Api, api_or_skip, offsets_live};
use modforge::client;
use serde_json::json;

const INPUT_CLASSES: [&str; 5] = [
    "BP_SGKController_C",
    "PlayerController",
    "EnhancedPlayerInput",
    "PlayerInput",
    "GameViewportClient",
];

fn field_offset(api: &Api, class: &str, field: &str) -> u64 {
    let response = api.op("discover_class_detail", json!({"name": class}));
    assert!(
        response.ok,
        "could not inspect {class}: {:?}",
        response.error
    );
    response.result["fields"]
        .as_array()
        .and_then(|fields| fields.iter().find(|candidate| candidate["name"] == field))
        .and_then(|field| field["offset"].as_u64())
        .unwrap_or_else(|| panic!("{class} has no reflected {field} field"))
}

fn input_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["input", "key", "action", "axis", "mouse"]
        .into_iter()
        .any(|word| name.contains(word))
}

fn print_input_functions(api: &Api, class: &str) {
    let response = api.op("class_functions_by_name", json!({"class": class}));
    assert!(
        response.ok,
        "could not inspect {class} functions: {:?}",
        response.error
    );
    let functions = response.result["functions"]
        .as_array()
        .unwrap_or_else(|| panic!("{class} returned no function list"));
    let relevant = functions
        .iter()
        .filter(|function| function["name"].as_str().is_some_and(input_name))
        .collect::<Vec<_>>();
    println!("{class} input functions:");
    for function in relevant {
        println!("  {function}");
        let Some(name) = function["name"].as_str() else {
            continue;
        };
        let parameters = api.op(
            "function_parameters",
            json!({"class": class, "function": name}),
        );
        if parameters.ok {
            println!("    parameters={}", parameters.result);
        }
    }
}

#[test]
#[ignore = "inspects the live MISERY player input implementation"]
fn inspect_real_player_keyboard_input() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player")
        .expect("the current-world MISERY player is not retained");
    let controller_offset = field_offset(&api, "Pawn", "Controller");
    let controller = client::read_u64(&api, player.addr, controller_offset);
    assert_ne!(controller, 0, "the retained player has no controller");
    let player_input_offset = field_offset(&api, "PlayerController", "PlayerInput");
    let player_input = client::read_u64(&api, controller, player_input_offset);
    assert_ne!(player_input, 0, "the player controller has no PlayerInput");

    println!(
        "player={} controller=0x{controller:X} Controller+0x{controller_offset:X}",
        player.full_name
    );
    println!("player_input=0x{player_input:X} PlayerInput+0x{player_input_offset:X}");

    for class in INPUT_CLASSES {
        print_input_functions(&api, class);
    }

    for structure in ["InputKeyParams", "Key"] {
        let detail = api.op("discover_struct_detail", json!({"name": structure}));
        assert!(
            detail.ok,
            "could not inspect {structure}: {:?}",
            detail.error
        );
        println!("{structure}={}", detail.result);
    }
}
