//! Research how the live MISERY player receives keyboard input.
//!
//! Pure discovery: finds the player's controller, PlayerInput object,
//! InputComponent, key-to-action mappings, and key state data, then
//! prints everything for manual analysis. No bot input, no writes.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 k3sc cargo-lock test -p misery-mod \
//!   --test research_player_input -- --ignored --test-threads=1 --nocapture
//! ```

mod common;

use common::{Api, api_or_skip, offsets_live};
use modforge::client;
use serde_json::json;

// --- helpers --------------------------------------------------------

fn field_offset(api: &Api, class: &str, field: &str) -> Option<u64> {
    let r = api.op("discover_class_detail", json!({"name": class}));
    if !r.ok {
        println!("  discover_class_detail({class}) failed: {:?}", r.error);
        return None;
    }
    r.result["fields"]
        .as_array()
        .and_then(|fields| fields.iter().find(|c| c["name"] == field))
        .and_then(|f| f["offset"].as_u64())
}

fn print_all_fields(api: &Api, class: &str) {
    let r = api.op("discover_class_detail", json!({"name": class}));
    if !r.ok {
        println!("  discover_class_detail({class}) failed: {:?}", r.error);
        return;
    }
    let Some(fields) = r.result["fields"].as_array() else {
        println!("  {class}: no fields array");
        return;
    };
    println!("{class} fields ({} total):", fields.len());
    for f in fields {
        let name = f["name"].as_str().unwrap_or("?");
        let ty = f["type"].as_str().unwrap_or("?");
        let offset = f["offset"].as_u64().unwrap_or(0);
        println!("  +0x{offset:X} {ty} {name}");
    }
}

fn print_all_functions(api: &Api, class: &str) {
    let r = api.op("class_functions_by_name", json!({"class": class}));
    if !r.ok {
        println!("  class_functions_by_name({class}) failed: {:?}", r.error);
        return;
    }
    let Some(functions) = r.result["functions"].as_array() else {
        println!("  {class}: no functions array");
        return;
    };
    println!("{class} functions ({} total):", functions.len());
    for f in functions {
        let name = f["name"].as_str().unwrap_or("?");
        let parms = f["parms_size"].as_u64().unwrap_or(0);
        let num = f["num_parms"].as_u64().unwrap_or(0);
        println!("  {name} ({num} parms, {parms} bytes)");
    }
}

fn print_function_parameters(api: &Api, class: &str, function: &str) {
    let r = api.op(
        "function_parameters",
        json!({"class": class, "function": function}),
    );
    if r.ok {
        println!("  {class}::{function} parameters={}", r.result);
    } else {
        println!("  {class}::{function} parameters FAILED: {:?}", r.error);
    }
}

fn read_object_class_name(api: &Api, addr: u64) -> String {
    let class_ptr = client::read_u64(api, addr, 0x10);
    if class_ptr == 0 {
        return "<no class>".into();
    }
    client::object_name(api, class_ptr).unwrap_or_else(|| "<unreadable>".into())
}

fn hex_dump(api: &Api, addr: u64, offset: u64, len: u64, label: &str) {
    let bytes = client::read_bytes(api, addr, offset, len);
    if bytes.is_empty() {
        println!("{label}: read failed");
        return;
    }
    println!("{label} ({} bytes from 0x{addr:X}+0x{offset:X}):", bytes.len());
    for (i, chunk) in bytes.chunks(16).enumerate() {
        let hex_str: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        println!("  {:04x}: {}", i * 16, hex_str.join(" "));
    }
}

// --- tests ----------------------------------------------------------

#[test]
#[ignore = "inspects the live MISERY player input implementation"]
fn discover_player_input_chain() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    // 1. find the live player
    let player = client::resolve_selector(&api, "live_player")
        .expect("the current-world MISERY player is not retained");
    println!("player: {} at 0x{:X}", player.full_name, player.addr);

    // 2. find the controller
    let controller_offset = field_offset(&api, "Pawn", "Controller")
        .expect("Pawn has no Controller field");
    let controller = client::read_u64(&api, player.addr, controller_offset);
    assert_ne!(controller, 0, "the retained player has no controller");
    let controller_class = read_object_class_name(&api, controller);
    println!(
        "controller: 0x{controller:X} class={controller_class} (Pawn+0x{controller_offset:X})"
    );

    // 3. find the PlayerInput object
    let player_input_offset = field_offset(&api, "PlayerController", "PlayerInput")
        .expect("PlayerController has no PlayerInput field");
    let player_input = client::read_u64(&api, controller, player_input_offset);
    assert_ne!(player_input, 0, "the player controller has no PlayerInput");
    let pi_class = read_object_class_name(&api, player_input);
    println!(
        "player_input: 0x{player_input:X} class={pi_class} (Controller+0x{player_input_offset:X})"
    );

    // 4. find the InputComponent on the player actor
    let ic_offset = field_offset(&api, "Actor", "InputComponent");
    if let Some(ic_off) = ic_offset {
        let input_comp = client::read_u64(&api, player.addr, ic_off);
        if input_comp != 0 {
            let ic_class = read_object_class_name(&api, input_comp);
            println!(
                "input_component: 0x{input_comp:X} class={ic_class} (Actor+0x{ic_off:X})"
            );
        } else {
            println!("input_component: null (Actor+0x{ic_off:X})");
        }
    } else {
        println!("input_component: Actor has no reflected InputComponent field");
    }

    println!("\n=== Controller class detail ===");
    print_all_fields(&api, &controller_class);
    print_all_functions(&api, &controller_class);

    println!("\n=== PlayerInput class detail ===");
    print_all_fields(&api, &pi_class);
    print_all_functions(&api, &pi_class);
}

#[test]
#[ignore = "reads the PlayerInput object's key state and mapping data"]
fn read_player_input_data() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player")
        .expect("the current-world MISERY player is not retained");

    let controller_offset = field_offset(&api, "Pawn", "Controller")
        .expect("Pawn has no Controller field");
    let controller = client::read_u64(&api, player.addr, controller_offset);
    assert_ne!(controller, 0, "no controller");

    let pi_offset = field_offset(&api, "PlayerController", "PlayerInput")
        .expect("no PlayerInput field");
    let player_input = client::read_u64(&api, controller, pi_offset);
    assert_ne!(player_input, 0, "no PlayerInput");

    let pi_class = read_object_class_name(&api, player_input);
    println!("PlayerInput at 0x{player_input:X} class={pi_class}");

    // dump the first 0x400 bytes of the PlayerInput object to find
    // TArray and TMap headers by their characteristic pointer+num+max
    // layout. this is the raw data the next test will decode.
    hex_dump(&api, player_input, 0, 0x400, "PlayerInput raw 0x000..0x400");

    // if EnhancedPlayerInput, also dump 0x400..0x800 (the enhanced
    // subclass adds its own fields after the base)
    if pi_class.contains("Enhanced") {
        hex_dump(
            &api,
            player_input,
            0x400,
            0x400,
            "PlayerInput raw 0x400..0x800",
        );
    }

    // try to find well-known field offsets on the base PlayerInput class
    println!("\n=== PlayerInput reflected fields ===");
    print_all_fields(&api, "PlayerInput");

    if pi_class.contains("Enhanced") {
        println!("\n=== EnhancedPlayerInput reflected fields ===");
        print_all_fields(&api, "EnhancedPlayerInput");
    }
}

#[test]
#[ignore = "reads the legacy ActionMappings and AxisMappings arrays"]
fn read_input_mappings() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player")
        .expect("no live player");

    let controller = client::read_u64(
        &api,
        player.addr,
        field_offset(&api, "Pawn", "Controller").expect("no Controller field"),
    );
    assert_ne!(controller, 0, "no controller");

    let player_input = client::read_u64(
        &api,
        controller,
        field_offset(&api, "PlayerController", "PlayerInput").expect("no PlayerInput field"),
    );
    assert_ne!(player_input, 0, "no PlayerInput");

    // try legacy PlayerInput fields: ActionMappings, AxisMappings
    for field in ["ActionMappings", "AxisMappings"] {
        let offset = field_offset(&api, "PlayerInput", field);
        match offset {
            Some(off) => {
                let (data, num, max) = client::read_tarray(&api, player_input, off);
                println!(
                    "{field}: offset=0x{off:X} data=0x{data:X} num={num} max={max}"
                );
                if num > 0 && data != 0 {
                    let read_len = (num * 0x30).min(0x400) as u64;
                    hex_dump(&api, data, 0, read_len, &format!("{field} entries"));
                }
            }
            None => println!("{field}: not reflected on PlayerInput"),
        }
    }

    // try EnhancedPlayerInput fields
    for field in [
        "EnhancedActionMappings",
        "CurrentInputActions",
        "ActionInstanceData",
        "InputActionInstances",
        "LastInjectedActions",
    ] {
        let offset = field_offset(&api, "EnhancedPlayerInput", field);
        match offset {
            Some(off) => {
                let (data, num, max) = client::read_tarray(&api, player_input, off);
                println!(
                    "{field}: offset=0x{off:X} data=0x{data:X} num={num} max={max}"
                );
                if num > 0 && data != 0 {
                    let read_len = (num * 0x40).min(0x800) as u64;
                    hex_dump(&api, data, 0, read_len, &format!("{field} entries"));
                }
            }
            None => println!("{field}: not reflected on EnhancedPlayerInput"),
        }
    }
}

#[test]
#[ignore = "reads the InputComponent bindings on the player actor"]
fn read_input_component() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player")
        .expect("no live player");

    let ic_offset = field_offset(&api, "Actor", "InputComponent");
    let Some(ic_off) = ic_offset else {
        println!("Actor has no reflected InputComponent field");
        return;
    };
    let input_comp = client::read_u64(&api, player.addr, ic_off);
    if input_comp == 0 {
        println!("InputComponent is null");
        return;
    }

    let ic_class = read_object_class_name(&api, input_comp);
    println!("InputComponent: 0x{input_comp:X} class={ic_class}");

    print_all_fields(&api, &ic_class);
    print_all_functions(&api, &ic_class);

    // dump raw bytes to find action and axis binding arrays
    hex_dump(
        &api,
        input_comp,
        0,
        0x300,
        "InputComponent raw 0x000..0x300",
    );

    // try known fields on base InputComponent
    for field in [
        "ActionBindings",
        "AxisBindings",
        "AxisKeyBindings",
        "KeyBindings",
        "TouchBindings",
        "GestureBindings",
    ] {
        let offset = field_offset(&api, "InputComponent", field);
        match offset {
            Some(off) => {
                let (data, num, max) = client::read_tarray(&api, input_comp, off);
                println!(
                    "{field}: offset=0x{off:X} data=0x{data:X} num={num} max={max}"
                );
                if num > 0 && data != 0 {
                    let read_len = (num * 0x40).min(0x400) as u64;
                    hex_dump(&api, data, 0, read_len, &format!("{field} entries"));
                }
            }
            None => println!("{field}: not reflected on InputComponent"),
        }
    }

    // if it is EnhancedInputComponent, try its fields too
    if ic_class.contains("Enhanced") {
        print_all_fields(&api, "EnhancedInputComponent");
        for field in [
            "EnhancedActionBindings",
            "InputActionBindings",
        ] {
            let offset = field_offset(&api, "EnhancedInputComponent", field);
            match offset {
                Some(off) => {
                    let (data, num, max) = client::read_tarray(&api, input_comp, off);
                    println!(
                        "{field}: offset=0x{off:X} data=0x{data:X} num={num} max={max}"
                    );
                    if num > 0 && data != 0 {
                        let read_len = (num * 0x60).min(0x800) as u64;
                        hex_dump(&api, data, 0, read_len, &format!("{field} entries"));
                    }
                }
                None => println!("{field}: not reflected on EnhancedInputComponent"),
            }
        }
    }
}

#[test]
#[ignore = "dumps the controller's PlayerInput functions with parameter layouts"]
fn dump_input_function_parameters() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let classes = [
        "PlayerInput",
        "EnhancedPlayerInput",
        "PlayerController",
        "BP_SGKController_C",
        "InputComponent",
        "EnhancedInputComponent",
    ];

    let input_keywords = [
        "input", "key", "action", "axis", "mouse", "press", "release",
        "held", "touch", "inject", "trigger", "bind", "mapping",
    ];

    for class in classes {
        let r = api.op("class_functions_by_name", json!({"class": class}));
        if !r.ok {
            println!("{class}: class_functions_by_name failed");
            continue;
        }
        let Some(functions) = r.result["functions"].as_array() else {
            continue;
        };
        let relevant: Vec<_> = functions
            .iter()
            .filter(|f| {
                f["name"].as_str().is_some_and(|n| {
                    let lower = n.to_ascii_lowercase();
                    input_keywords.iter().any(|kw| lower.contains(kw))
                })
            })
            .collect();
        if relevant.is_empty() {
            println!("{class}: no input-related functions");
            continue;
        }
        println!("\n{class} input-related functions ({}):", relevant.len());
        for f in &relevant {
            let name = f["name"].as_str().unwrap_or("?");
            println!("  {name}");
            print_function_parameters(&api, class, name);
        }
    }

    // also try to find the struct layouts for input types
    let structs = [
        "InputActionKeyMapping",
        "InputAxisKeyMapping",
        "InputKeyParams",
        "Key",
        "FEnhancedActionKeyMapping",
        "EnhancedActionKeyMapping",
        "InputActionValue",
    ];
    println!("\n=== Input struct layouts ===");
    for s in structs {
        let r = api.op("discover_struct_detail", json!({"name": s}));
        if r.ok {
            println!("{s}: {}", r.result);
        } else {
            println!("{s}: not found");
        }
    }
}

#[test]
#[ignore = "resolves EnhancedActionMappings action names and key names"]
fn decode_enhanced_action_mappings() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(30));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player")
        .expect("no live player");
    let controller = client::read_u64(
        &api,
        player.addr,
        field_offset(&api, "Pawn", "Controller").expect("no Controller field"),
    );
    let player_input = client::read_u64(
        &api,
        controller,
        field_offset(&api, "PlayerController", "PlayerInput").expect("no PlayerInput field"),
    );
    assert_ne!(player_input, 0, "no PlayerInput");

    let eam_offset = field_offset(&api, "EnhancedPlayerInput", "EnhancedActionMappings")
        .expect("no EnhancedActionMappings field");
    let (data, num, _max) = client::read_tarray(&api, player_input, eam_offset);
    println!("EnhancedActionMappings: {num} entries at 0x{data:X}");

    if num == 0 || data == 0 {
        println!("no mappings to decode");
        return;
    }

    // we do not know the entry stride. try several candidates and pick
    // the one where the most entry[i]+0x00 values resolve to valid
    // UObject names (the UInputAction pointer).
    //
    // from UE source, FEnhancedActionKeyMapping contains:
    //   UInputAction* Action (8)
    //   FKey Key (24: FName + pad)
    //   ... triggers, modifiers, bools ...
    // typical sizes: 0x48, 0x50, 0x58, 0x60, 0x68, 0x70, 0x78, 0x80

    let max_read: u64 = 0x6000;
    let raw = client::read_bytes(&api, data, 0, max_read);
    println!("read {} bytes of mapping data", raw.len());

    // find the stride by checking which candidate gives the most
    // valid action pointers across all entries
    let candidates: &[usize] = &[0x38, 0x40, 0x48, 0x50, 0x58, 0x60, 0x68, 0x70, 0x78, 0x80];
    let mut best_stride = 0x70usize;
    let mut best_hits = 0usize;

    for &stride in candidates {
        let mut hits = 0;
        for i in 0..num.min(20) {
            let off = i * stride;
            if off + 8 > raw.len() { break; }
            let ptr = u64::from_le_bytes(raw[off..off + 8].try_into().unwrap());
            if ptr > 0x100_0000_0000 && ptr < 0x800_0000_0000_0000 {
                hits += 1;
            }
        }
        println!("  stride 0x{stride:02X}: {hits}/{} entries look like pointers", num.min(20));
        if hits > best_hits {
            best_hits = hits;
            best_stride = stride;
        }
    }
    println!("best stride: 0x{best_stride:02X} ({best_hits} hits)\n");

    println!("{:>4} {:>20} {:>20} {:>24}", "#", "action_ptr", "action_name", "key_name");
    println!("{}", "-".repeat(72));

    for i in 0..num {
        let off = i * best_stride;
        if off + 24 > raw.len() {
            break;
        }
        let action_ptr = u64::from_le_bytes(raw[off..off + 8].try_into().unwrap());
        let action_name = if action_ptr > 0x100_0000_0000 && action_ptr < 0x800_0000_0000_0000 {
            client::object_name(&api, action_ptr).unwrap_or_else(|| "<unreadable>".into())
        } else if action_ptr == 0 {
            "<null>".into()
        } else {
            format!("<bad:0x{action_ptr:X}>")
        };

        // FKey starts at +0x08. FKey contains FName at its start (8 bytes).
        let key_fname = u64::from_le_bytes(raw[off + 8..off + 16].try_into().unwrap());
        let key_name = if key_fname != 0 && key_fname < 0x1_0000_0000 {
            client::fname_to_string(&api, key_fname)
                .unwrap_or_else(|| format!("fname:0x{key_fname:X}"))
        } else if key_fname == 0 {
            "<none>".into()
        } else {
            format!("<bad:0x{key_fname:X}>")
        };

        println!("{i:4} 0x{action_ptr:016X} {action_name:>20} {key_name:>24}");
    }

    // also try reading the Mapping Context's own Mappings TArray
    let mc_offset = field_offset(&api, "BP_SGKController_C", "Mapping Context");
    if let Some(mc_off) = mc_offset {
        let mc_ptr = client::read_u64(&api, controller, mc_off);
        if mc_ptr != 0 {
            let mc_name = client::object_name(&api, mc_ptr)
                .unwrap_or_else(|| "<unreadable>".into());
            println!("\nMapping Context: 0x{mc_ptr:X} name={mc_name}");

            // Mappings TArray at +0x30 inside InputMappingContext
            let mappings_tarray = client::read_bytes(&api, mc_ptr, 0x30, 16);
            if mappings_tarray.len() >= 16 {
                let mc_data = u64::from_le_bytes(mappings_tarray[0..8].try_into().unwrap());
                let mc_num = u32::from_le_bytes(mappings_tarray[8..12].try_into().unwrap()) as usize;
                println!("  InputMappingContext.Mappings: {mc_num} entries at 0x{mc_data:X}");

                if mc_num > 0 && mc_num < 500 && mc_data > 0x100_0000_0000 {
                    // stride is 0x50. action pointer at +0x20, key FName at +0x28.
                    let stride: usize = 0x50;
                    let total = mc_num * stride;
                    let mc_raw = client::read_bytes(&api, mc_data, 0, total as u64);
                    println!("  read {} bytes for {} entries at stride 0x{stride:X}", mc_raw.len(), mc_num);

                    println!("\n  {:>3} {:>30} {:>30}", "#", "action", "key");
                    println!("  {}", "-".repeat(66));

                    let mut seen = std::collections::HashMap::<String, Vec<String>>::new();

                    for i in 0..mc_num {
                        let off = i * stride;
                        if off + 0x30 > mc_raw.len() { break; }

                        let action_ptr = u64::from_le_bytes(mc_raw[off + 0x20..off + 0x28].try_into().unwrap());
                        let key_fname = u64::from_le_bytes(mc_raw[off + 0x28..off + 0x30].try_into().unwrap());

                        let action_name = if action_ptr > 0x1_0000_0000 && action_ptr < 0x800_0000_0000_0000 {
                            client::object_name(&api, action_ptr).unwrap_or_else(|| format!("0x{action_ptr:X}"))
                        } else if action_ptr == 0 {
                            "<null>".into()
                        } else {
                            format!("<bad:0x{action_ptr:X}>")
                        };

                        let key_name = if key_fname != 0 && key_fname < 0x1_0000_0000 {
                            client::fname_to_string(&api, key_fname)
                                .unwrap_or_else(|| format!("fname:0x{key_fname:X}"))
                        } else if key_fname == 0 {
                            "<none>".into()
                        } else {
                            format!("<bad:0x{key_fname:X}>")
                        };

                        println!("  {i:3} {action_name:>30} {key_name:>30}");
                        seen.entry(action_name.clone()).or_default().push(key_name.clone());
                    }

                    // summary: unique actions and their bound keys
                    println!("\n  === action-to-key summary ===");
                    let mut actions: Vec<_> = seen.iter().collect();
                    actions.sort_by_key(|(name, _)| name.clone());
                    for (action, keys) in &actions {
                        println!("  {action}: {}", keys.join(", "));
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "snapshots key state fields on EnhancedPlayerInput while idle vs holding W"]
fn observe_key_state_fields() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(30));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player")
        .expect("no live player");
    let controller = client::read_u64(
        &api,
        player.addr,
        field_offset(&api, "Pawn", "Controller").expect("no Controller field"),
    );
    assert!(controller != 0, "controller is null");

    let epi_offset = field_offset(&api, "PlayerController", "PlayerInput")
        .expect("no PlayerInput field");
    let epi = client::read_u64(&api, controller, epi_offset);
    assert!(epi != 0, "EnhancedPlayerInput is null");
    println!("EnhancedPlayerInput: 0x{epi:X}");

    let dump_range: u64 = 0xA00;
    let snap1 = client::read_bytes(&api, epi, 0, dump_range);
    println!("snapshot 1 (idle): {} bytes from EnhancedPlayerInput", snap1.len());

    let tarray_offsets: &[(&str, u64)] = &[
        ("EnhancedActionMappings", 0x538),
        ("ActionInstanceData", 0x598),
        ("KeysPressedThisTick", 0x688),
        ("InputsInjectedThisTick", 0x6D8),
    ];

    for &(name, off) in tarray_offsets {
        if (off + 16) as usize <= snap1.len() {
            let data = u64::from_le_bytes(snap1[off as usize..off as usize + 8].try_into().unwrap());
            let num = u32::from_le_bytes(snap1[off as usize + 8..off as usize + 12].try_into().unwrap());
            println!("  +0x{off:03X} {name}: {num} entries, data=0x{data:X}");
        }
    }

    fn dump_tarray(api: &Api, snap: &[u8], off: usize, label: &str) {
        let data = u64::from_le_bytes(snap[off..off + 8].try_into().unwrap());
        let num = u32::from_le_bytes(snap[off + 8..off + 12].try_into().unwrap()) as usize;
        if num > 0 && data > 0x1_0000_0000 {
            let raw = client::read_bytes(api, data, 0, (num * 64).min(0x800) as u64);
            println!("\n  {label} ({num} entries, {} bytes):", raw.len());
            for row in 0..(raw.len() / 16) {
                let o = row * 16;
                let hex: Vec<String> = raw[o..o + 16].iter().map(|b| format!("{b:02X}")).collect();
                println!("    +0x{o:04X}  {}", hex.join(" "));
            }
        } else {
            println!("\n  {label}: empty");
        }
    }

    dump_tarray(&api, &snap1, 0x688, "KeysPressedThisTick (idle)");
    dump_tarray(&api, &snap1, 0x6D8, "InputsInjectedThisTick (idle)");
    dump_tarray(&api, &snap1, 0x598, "ActionInstanceData (idle)");

    println!("\n=== HOLD W NOW and keep holding it ===");
    println!("(waiting 10 seconds before taking second snapshot)");
    std::thread::sleep(std::time::Duration::from_secs(10));

    let snap2 = client::read_bytes(&api, epi, 0, dump_range);
    println!("\nsnapshot 2 (holding W): {} bytes", snap2.len());

    println!("\n=== DIFF (changed 8-byte values) ===");
    let compare_len = snap1.len().min(snap2.len());
    for off in (0..compare_len).step_by(8) {
        if off + 8 > compare_len { break; }
        let v1 = u64::from_le_bytes(snap1[off..off + 8].try_into().unwrap());
        let v2 = u64::from_le_bytes(snap2[off..off + 8].try_into().unwrap());
        if v1 != v2 {
            println!("  +0x{off:03X}: 0x{v1:016X} -> 0x{v2:016X}");
        }
    }

    dump_tarray(&api, &snap2, 0x688, "KeysPressedThisTick (holding W)");
    dump_tarray(&api, &snap2, 0x6D8, "InputsInjectedThisTick (holding W)");
    dump_tarray(&api, &snap2, 0x598, "ActionInstanceData (holding W)");

    // try to resolve FNames in KeysPressedThisTick
    let kp_data2 = u64::from_le_bytes(snap2[0x688..0x690].try_into().unwrap());
    let kp_num2 = u32::from_le_bytes(snap2[0x690..0x694].try_into().unwrap()) as usize;
    if kp_num2 > 0 && kp_data2 > 0x1_0000_0000 {
        let kp_raw2 = client::read_bytes(&api, kp_data2, 0, (kp_num2 * 64).min(0x400) as u64);
        println!("\n  resolving KeysPressedThisTick FNames:");
        for i in 0..kp_num2.min(20) {
            // try every 8-byte value as a possible FName
            for fname_off in (0..64).step_by(8) {
                let entry_off = i * 64 + fname_off;
                if entry_off + 8 > kp_raw2.len() { break; }
                let fname = u64::from_le_bytes(kp_raw2[entry_off..entry_off + 8].try_into().unwrap());
                if fname != 0 && fname < 0x1_0000_0000 {
                    if let Some(name) = client::fname_to_string(&api, fname) {
                        println!("    entry[{i}]+0x{fname_off:02X}: {name}");
                    }
                }
            }
        }
    }

    println!("\ndone");
}

#[test]
#[ignore = "decodes UPlayerInput KeyStateMap to find per-key pressed state"]
fn decode_key_state_map() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(30));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player")
        .expect("no live player");
    let controller = client::read_u64(
        &api,
        player.addr,
        field_offset(&api, "Pawn", "Controller").expect("no Controller field"),
    );
    assert!(controller != 0, "controller is null");

    let epi_offset = field_offset(&api, "PlayerController", "PlayerInput")
        .expect("no PlayerInput field");
    let epi = client::read_u64(&api, controller, epi_offset);
    assert!(epi != 0, "EnhancedPlayerInput is null");
    println!("EnhancedPlayerInput: 0x{epi:X}");

    // list all fields on PlayerInput (the parent class)
    println!("\n--- PlayerInput fields ---");
    print_all_fields(&api, "PlayerInput");

    println!("\n--- EnhancedPlayerInput fields ---");
    print_all_fields(&api, "EnhancedPlayerInput");

    // the KeyStateMap is not a UProperty (it's native C++ TMap),
    // so discover_class_detail won't list it. but we know from
    // the diff that it's in the range +0x5E0 to +0x650 on the
    // EnhancedPlayerInput object.
    //
    // a UE TMap<FKey, FKeyState> is stored as a TSortedMap or
    // TMap backed by a TSet. the inline layout is:
    //   - TSet::Elements (TArray of TSetElement<TPair<FKey,FKeyState>>)
    //     which is: data pointer (8), count (4), capacity (4)
    //   - TSet::Hash (hash bucket data)
    //
    // but UPlayerInput actually stores key state in a different
    // structure. let's just dump the region that changed and
    // look for recognizable patterns.

    // dump a wide region around where the changes happened
    // the diff showed changes at +0x2E8, +0x2F0, +0x308 (on ActionInstanceData)
    // and +0x5E8, +0x5F0, +0x5F8, +0x610, +0x648
    // those +0x5xx offsets are on the EPI object itself, not ActionInstanceData

    // first, let me check: those offsets in the diff are on the
    // EPI object. the ActionInstanceData is a separate allocation.
    // so +0x5E8 on EPI is a field ON the EPI object.

    // dump +0x400 to +0x700 to see all native fields between
    // the reflected fields
    let region = client::read_bytes(&api, epi, 0x400, 0x300);
    println!("\nEPI raw +0x400..+0x700 ({} bytes):", region.len());
    for row in 0..(region.len() / 16) {
        let off = row * 16;
        let abs_off = 0x400 + off;
        let hex: Vec<String> = region[off..off + 16].iter().map(|b| format!("{b:02X}")).collect();
        println!("  +0x{abs_off:03X}  {}", hex.join(" "));
    }

    // now look for TArray/TMap shaped data: pointer + count pairs
    println!("\n--- pointer+count scan in +0x400..+0x700 ---");
    for off in (0..region.len().saturating_sub(15)).step_by(8) {
        let val = u64::from_le_bytes(region[off..off + 8].try_into().unwrap());
        if val > 0x1_0000_0000 && val < 0x800_0000_0000_0000 {
            let count = u32::from_le_bytes(region[off + 8..off + 12].try_into().unwrap());
            let cap = u32::from_le_bytes(region[off + 12..off + 16].try_into().unwrap());
            if count > 0 && count < 10000 && cap >= count {
                let abs_off = 0x400 + off;
                println!("  +0x{abs_off:03X}: ptr=0x{val:X} count={count} cap={cap} (TArray?)");
            }
        }
    }

    // the real KeyStateMap might be the TMap at a known offset.
    // UPlayerInput in UE source has:
    //   TMap<FKey, FKeyState> KeyStateMap
    // a TMap in UE5 is: TSparseArray Elements + TInlineFreeList + HashTable
    // the TSparseArray starts with: data ptr, count, capacity
    // let's look for that pattern with a count matching the number
    // of keys that have been pressed since game start

    println!("\n=== HOLD W NOW and keep holding it ===");
    println!("(waiting 10 seconds)");
    std::thread::sleep(std::time::Duration::from_secs(10));

    let region2 = client::read_bytes(&api, epi, 0x400, 0x300);
    println!("\n--- DIFF +0x400..+0x700 (idle vs holding W) ---");
    for off in (0..region.len().min(region2.len())).step_by(8) {
        if off + 8 > region.len() || off + 8 > region2.len() { break; }
        let v1 = u64::from_le_bytes(region[off..off + 8].try_into().unwrap());
        let v2 = u64::from_le_bytes(region2[off..off + 8].try_into().unwrap());
        if v1 != v2 {
            let abs_off = 0x400 + off;
            println!("  +0x{abs_off:03X}: 0x{v1:016X} -> 0x{v2:016X}");
        }
    }

    // if we found a TArray-like structure, dump its contents
    // to see the FKey entries
    // check the known changed offsets for a data pointer
    for check_off in [0x5E0_u64, 0x5E8, 0x5F0, 0x5F8, 0x600, 0x608, 0x610] {
        let local = (check_off - 0x400) as usize;
        if local + 16 > region2.len() { continue; }
        let ptr = u64::from_le_bytes(region2[local..local + 8].try_into().unwrap());
        let count = u32::from_le_bytes(region2[local + 8..local + 12].try_into().unwrap());
        let cap = u32::from_le_bytes(region2[local + 12..local + 16].try_into().unwrap());
        if ptr > 0x1_0000_0000 && ptr < 0x800_0000_0000_0000 && count > 0 && count < 1000 && cap >= count {
            println!("\n  potential TArray at +0x{check_off:03X}: ptr=0x{ptr:X} count={count} cap={cap}");
            let content = client::read_bytes(&api, ptr, 0, (count as u64 * 64).min(0x800));
            println!("  content ({} bytes):", content.len());
            for row in 0..(content.len().min(0x200) / 16) {
                let o = row * 16;
                let hex: Vec<String> = content[o..o + 16].iter().map(|b| format!("{b:02X}")).collect();
                println!("    +0x{o:04X}  {}", hex.join(" "));
            }
            // resolve 8-byte values as UObject pointers
            println!("  UObject pointer scan:");
            for i in (0..content.len().min(0x200)).step_by(8) {
                let ptr = u64::from_le_bytes(content[i..i + 8].try_into().unwrap());
                if ptr > 0x1_0000_0000 && ptr < 0x800_0000_0000_0000 {
                    if let Some(name) = client::object_name(&api, ptr) {
                        println!("    +0x{i:04X}: 0x{ptr:X} = {name}");
                    }
                }
            }
        }
    }

    // now look deeper. the real key state map on UPlayerInput is
    // NOT a reflected field. UPlayerInput stores key state in
    // native TMap<FKey, FKeyState>. In UE5, UPlayerInput's native
    // layout before the reflected DebugExecBindings (+0x1A0) has:
    //   KeyStateMap, PressedKeys, etc.
    // the base class chain is:
    //   UObject -> UPlayerInput -> UEnhancedPlayerInput
    // UObject ends around +0x28. UPlayerInput adds its fields.
    // let's dump +0x28 to +0x200 to find the TMap.

    let low_region = client::read_bytes(&api, epi, 0x28, 0x1D8);
    println!("\n--- EPI low region +0x28..+0x200 ---");
    for row in 0..(low_region.len() / 16) {
        let off = row * 16;
        let abs_off = 0x28 + off;
        let hex: Vec<String> = low_region[off..off + 16].iter().map(|b| format!("{b:02X}")).collect();
        println!("  +0x{abs_off:03X}  {}", hex.join(" "));
    }

    // scan for TMap/TArray patterns in the low region
    println!("\n--- pointer+count scan in +0x28..+0x200 ---");
    for off in (0..low_region.len().saturating_sub(15)).step_by(8) {
        let val = u64::from_le_bytes(low_region[off..off + 8].try_into().unwrap());
        if val > 0x1_0000_0000 && val < 0x800_0000_0000_0000 {
            let count = u32::from_le_bytes(low_region[off + 8..off + 12].try_into().unwrap());
            let cap = u32::from_le_bytes(low_region[off + 12..off + 16].try_into().unwrap());
            if count > 0 && count < 10000 && cap >= count {
                let abs_off = 0x28 + off;
                println!("  +0x{abs_off:03X}: ptr=0x{val:X} count={count} cap={cap}");

                // if count looks like key count (dozens of keys), dump a few entries
                if count > 5 && count < 200 {
                    let arr = client::read_bytes(&api, val, 0, (count as u64 * 64).min(0x400));
                    println!("    first {} bytes:", arr.len());
                    for row in 0..(arr.len().min(0x100) / 16) {
                        let o = row * 16;
                        let hex: Vec<String> = arr[o..o + 16].iter().map(|b| format!("{b:02X}")).collect();
                        println!("      +0x{o:04X}  {}", hex.join(" "));
                    }
                    // try FName resolution on every 8 bytes
                    println!("    FName scan (first 0x100):");
                    for i in (0..arr.len().min(0x100)).step_by(8) {
                        let fname = u64::from_le_bytes(arr[i..i + 8].try_into().unwrap());
                        if fname != 0 && fname < 0x1_0000_0000 {
                            if let Some(name) = client::fname_to_string(&api, fname) {
                                println!("      +0x{i:04X}: {name}");
                            }
                        }
                    }
                }
            }
        }
    }

    println!("\ndone");
}

fn actor_location(api: &Api, selector: &str) -> [f64; 3] {
    let (out, _) = api
        .call_ufunction("Actor", "K2_GetActorLocation", selector, &[0u8; 0x18])
        .expect("K2_GetActorLocation failed");
    assert_eq!(out.len(), 0x18);
    [
        client::from_le_f64(&out, 0x00),
        client::from_le_f64(&out, 0x08),
        client::from_le_f64(&out, 0x10),
    ]
}

#[test]
#[ignore = "writes ForwardInput trigger state to make the player walk"]
fn inject_forward_input() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(30));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player")
        .expect("no live player");
    let controller = client::read_u64(
        &api,
        player.addr,
        field_offset(&api, "Pawn", "Controller").expect("no Controller field"),
    );
    assert!(controller != 0);

    let epi = client::read_u64(
        &api,
        controller,
        field_offset(&api, "PlayerController", "PlayerInput").expect("no PlayerInput field"),
    );
    assert!(epi != 0);
    println!("EnhancedPlayerInput: 0x{epi:X}");

    let aid_data = client::read_u64(&api, epi, 0x598);
    let aid_num = client::read_u64(&api, epi, 0x5A0) as u32 as usize;
    println!("ActionInstanceData: {aid_num} entries at 0x{aid_data:X}");

    let stride: usize = 0x70;
    let mut forward_idx: Option<usize> = None;
    for i in 0..aid_num {
        let ptr = client::read_u64(&api, aid_data, (i * stride) as u64);
        if let Some(name) = client::object_name(&api, ptr) {
            if name == "ForwardInput" {
                forward_idx = Some(i);
                println!("ForwardInput at entry {i}, ptr=0x{ptr:X}");
                break;
            }
        }
    }
    let fi = forward_idx.expect("ForwardInput not found in ActionInstanceData");
    let fi_addr = aid_data + (fi * stride) as u64;

    let sel = format!("addr:0x{:X}", player.addr);
    let pos_before = actor_location(&api, &sel);
    println!("position before: {:.1}, {:.1}, {:.1}", pos_before[0], pos_before[1], pos_before[2]);

    let start = std::time::Instant::now();
    let mut writes = 0u32;
    while start.elapsed() < std::time::Duration::from_secs(2) {
        client::write_bytes(&api, fi_addr, 0x10, &[0x02]);
        client::write_bytes(&api, fi_addr, 0x40, &1.0_f64.to_le_bytes());
        writes += 1;
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    println!("wrote trigger=2, value=1.0 ({writes} writes over 2s)");

    client::write_bytes(&api, fi_addr, 0x10, &[0x00]);
    client::write_bytes(&api, fi_addr, 0x40, &0.0_f64.to_le_bytes());
    println!("cleared ForwardInput");

    let pos_after = actor_location(&api, &sel);
    println!("position after:  {:.1}, {:.1}, {:.1}", pos_after[0], pos_after[1], pos_after[2]);

    let dx = pos_after[0] - pos_before[0];
    let dy = pos_after[1] - pos_before[1];
    let dz = pos_after[2] - pos_before[2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
    println!("distance moved: {dist:.1}");

    if dist > 1.0 {
        println!("SUCCESS: player moved {dist:.1} units");
    } else {
        println!("FAILED: player did not move (dist={dist:.4})");
    }

    // list available functions on input classes
    println!("\n--- PlayerInput functions ---");
    print_all_functions(&api, "PlayerInput");
    println!("\n--- EnhancedPlayerInput functions ---");
    print_all_functions(&api, "EnhancedPlayerInput");
    println!("\n--- PlayerController functions ---");
    print_all_functions(&api, "PlayerController");
}

#[test]
#[ignore = "watches the address a physical W press writes, records the writer + call chain"]
fn watch_forward_input_writer() {
    let Some(api) = api_or_skip() else { return };
    // The op blocks for the whole watch window; give the client
    // room past that before it times out.
    let api = api.with_timeout(std::time::Duration::from_secs(45));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let controller = client::read_u64(&api, player.addr, 0x2C8);
    assert_ne!(controller, 0, "no controller");
    let epi = client::read_u64(&api, controller, 0x408);
    assert_ne!(epi, 0, "no EnhancedPlayerInput");
    println!("EnhancedPlayerInput: 0x{epi:X}");

    // Find the ForwardInput entry in ActionInstanceData (+0x598,
    // stride 0x70). Its analog value (+0x40) is rewritten every
    // frame W is held; its trigger byte (+0x10) flips on press.
    let aid_data = client::read_u64(&api, epi, 0x598);
    let aid_num = client::read_u64(&api, epi, 0x5A0) as u32 as usize;
    assert_ne!(aid_data, 0, "no ActionInstanceData");
    let stride: usize = 0x70;
    let mut forward_idx: Option<usize> = None;
    for i in 0..aid_num {
        let ptr = client::read_u64(&api, aid_data, (i * stride) as u64);
        if client::object_name(&api, ptr).as_deref() == Some("ForwardInput") {
            forward_idx = Some(i);
            break;
        }
    }
    let fi = forward_idx.expect("ForwardInput not found in ActionInstanceData");
    let entry = aid_data + (fi * stride) as u64;
    println!("ForwardInput entry: 0x{entry:X}");

    // Candidate write targets. These are written on the key-DOWN
    // transition, not continuously while held, so the writer is only
    // caught if W is pressed fresh during the window. We sweep all of
    // them in one session.
    let candidates: &[(&str, u64, u8)] = &[
        ("trigger", entry + 0x10, 1), // trigger state byte, flips on press
        ("value", entry + 0x40, 8),   // f64 analog value, 0.0 <-> 1.0
        ("epi5e8", epi + 0x5E8, 8),   // KeyStateMap region (written inside InputKey)
        ("epi5f0", epi + 0x5F0, 8),
    ];

    println!("\ncandidate targets:");
    for (n, a, l) in candidates {
        println!("  {n:8} 0x{a:X} len={l}");
    }

    let secs: u64 = std::env::var("WATCH_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);

    println!(
        "\n=== TAP W repeatedly now (press and release, over and over) ===\n\
         Keep tapping for the whole test (~{}s total across {} targets).\n",
        secs * candidates.len() as u64,
        candidates.len(),
    );

    for (name, addr, len) in candidates {
        println!("\n########## watching {name} at 0x{addr:X} (len {len}) for {secs}s ##########");
        let r = api.op(
            "watch_writes",
            json!({"addr": addr, "len": len, "duration_ms": secs * 1000}),
        );
        if !r.ok {
            println!("watch_writes FAILED: {:?}", r.error);
            continue;
        }
        let res = &r.result;
        println!(
            "threads armed={} hit_count={} arm_diag={}",
            res["threads_armed"], res["hit_count"], res["arm_diag"],
        );

        let Some(records) = res["records"].as_array() else {
            println!("  no records array");
            continue;
        };
        if records.is_empty() {
            println!("  NO WRITES CAUGHT for {name}.");
            continue;
        }

        println!("  {} write(s) captured:", records.len());
        for (i, rec) in records.iter().enumerate() {
            println!("  --- write #{i} (tid {}) ---", rec["tid"]);
            println!(
                "    writing instruction (rip after write): {} = {}+{}",
                rec["rip_after_write"].as_str().unwrap_or("?"),
                rec["rip_module"].as_str().unwrap_or("?"),
                rec["rip_rva"].as_str().unwrap_or("?"),
            );
            if let Some(frames) = rec["stack_return_addrs"].as_array() {
                println!("    call chain (return addresses in exe .text, nearest first):");
                for f in frames {
                    println!(
                        "      {} = {}+{}",
                        f["addr"].as_str().unwrap_or("?"),
                        f["module"].as_str().unwrap_or("?"),
                        f["rva"].as_str().unwrap_or("?"),
                    );
                }
            }
        }
    }
    println!(
        "\ndone. Map each rip and its nearest return addresses (exe RVA) to \
         functions to identify InputKey and its callers."
    );
}

#[test]
#[ignore = "names the functions in the captured ForwardInput write call chain"]
fn identify_forward_input_writers() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    // Fetch the exe base from a 1 ms throwaway watch on a valid,
    // aligned address (the live player's EnhancedPlayerInput).
    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let controller = client::read_u64(&api, player.addr, 0x2C8);
    let epi = client::read_u64(&api, controller, 0x408);
    assert_ne!(epi, 0, "no EnhancedPlayerInput");
    let probe = api.op("watch_writes", json!({"addr": epi, "len": 8, "duration_ms": 1}));
    let base_str = probe.result["exe_base"].as_str().expect("no exe_base");
    let base = u64::from_str_radix(base_str.trim_start_matches("0x"), 16).expect("bad exe_base");
    println!("exe base: 0x{base:X}");

    // RVAs to identify. Default is the ForwardInput trigger-write
    // chain; override with WATCH_RVAS (comma-separated hex, first is
    // the writing instruction) to identify any other captured chain.
    let default_writer: u64 = 0x42f14d2;
    let default_chain: Vec<u64> =
        vec![0xf590f8, 0xf4dd41, 0xf6682d, 0x11af012, 0x7bf8140, 0x7be5918, 0x3cb9443];
    let (writer_rva, chain_owned): (u64, Vec<u64>) = match std::env::var("WATCH_RVAS") {
        Ok(s) => {
            let v: Vec<u64> = s
                .split(',')
                .filter_map(|t| u64::from_str_radix(t.trim().trim_start_matches("0x"), 16).ok())
                .collect();
            (v[0], v[1..].to_vec())
        }
        Err(_) => (default_writer, default_chain),
    };
    let chain_rvas: &[u64] = &chain_owned;

    // APlayerController::InputKey's prologue, from the earlier scan.
    let inputkey_sig: [u8; 21] = [
        0x40, 0x55, 0x57, 0x41, 0x57, 0x48, 0x8d, 0x6c, 0x24, 0xb9, 0x48, 0x81, 0xec, 0xf0, 0x00,
        0x00, 0x00, 0xf6, 0x41, 0x58, 0x10,
    ];

    // Read backward from an address to the function entry (the byte
    // after the preceding run of 0xCC int3 padding). Returns the
    // entry's absolute address and its first 24 bytes.
    let fn_entry = |addr: u64| -> Option<(u64, Vec<u8>)> {
        let window: u64 = 0x1200;
        let start = addr.saturating_sub(window);
        let bytes = client::read_bytes(&api, start, 0, addr - start);
        if bytes.is_empty() {
            return None;
        }
        for i in (1..bytes.len()).rev() {
            if bytes[i - 1] == 0xCC && bytes[i] != 0xCC {
                let entry = start + i as u64;
                let prologue = client::read_bytes(&api, entry, 0, 24);
                return Some((entry, prologue));
            }
        }
        None
    };

    let report = |label: &str, addr_rva: u64| {
        let addr = base + addr_rva;
        match fn_entry(addr) {
            Some((entry, prologue)) => {
                let entry_rva = entry - base;
                let hex: String = prologue.iter().map(|b| format!("{b:02x} ")).collect();
                let is_inputkey = prologue.len() >= 21 && prologue[..21] == inputkey_sig;
                let mark = if is_inputkey { "  <== InputKey prologue MATCH" } else { "" };
                println!(
                    "{label:8} rva=+0x{addr_rva:<8x} fn_entry=+0x{entry_rva:<8x} \
                     prologue: {hex}{mark}"
                );
            }
            None => println!("{label:8} rva=+0x{addr_rva:<8x} fn_entry: not found"),
        }
    };

    println!("\n=== function containing each captured address ===");
    report("writer", writer_rva);
    for (i, &rva) in chain_rvas.iter().enumerate() {
        report(&format!("frame{i}"), rva);
    }

    // Cross-check: what does the prologue scan resolve to this session?
    println!("\n=== find_inputkey prologue scan (this session) ===");
    let r = api.op("find_inputkey", json!({}));
    if let Some(hits) = r.result["prologue_scan"]["hits"].as_array() {
        for h in hits {
            if let Some(s) = h["fn_addr"].as_str() {
                if let Ok(a) = u64::from_str_radix(s.trim_start_matches("0x"), 16) {
                    println!("  prologue-scan hit 0x{a:X} = +0x{:x}", a.wrapping_sub(base));
                }
            }
        }
    } else {
        println!("  no prologue-scan hits");
    }
}

/// Find the candidate KeyStateMap-style blocks on the EPI object:
/// TSet blocks whose entries begin with an FKey (FName + FKeyDetails
/// pointer) and that contain W. Returns (header_offset, w_entry_addr).
fn keystatemap_w_entries(api: &Api, epi: u64, w_idx: u32) -> Vec<(u64, u64)> {
    let obj = client::read_bytes(api, epi, 0x28, 0x800);
    let heap_lo = 0x1_0000_0000u64;
    let heap_hi = 0x8000_0000_0000u64;
    let mut out = Vec::new();
    for off in (0..obj.len().saturating_sub(16)).step_by(8) {
        let ptr = u64::from_le_bytes(obj[off..off + 8].try_into().unwrap());
        let count = u32::from_le_bytes(obj[off + 8..off + 12].try_into().unwrap());
        let cap = u32::from_le_bytes(obj[off + 12..off + 16].try_into().unwrap());
        if ptr < heap_lo || ptr >= heap_hi || count == 0 || count > 1000 || cap < count || cap > 8192
        {
            continue;
        }
        let block = client::read_bytes(api, ptr, 0, ((cap as u64) * 0x80).min(0x8000));
        let mut b = 0usize;
        while b + 12 <= block.len() {
            let v = u32::from_le_bytes(block[b..b + 4].try_into().unwrap());
            if v == w_idx {
                // FKey layout: FName(8) then TSharedPtr<FKeyDetails> (a
                // heap pointer at +0x08). Require it so we skip the
                // action-mapping blocks whose FKeys have null details.
                let details = u64::from_le_bytes(block[b + 8..b + 16].try_into().unwrap());
                if details >= heap_lo && details < heap_hi {
                    out.push((0x28 + off as u64, ptr + b as u64));
                }
            }
            b += 4;
        }
    }
    out
}

#[test]
#[ignore = "write-watches W's FKeyState to capture InputKey (the real writer) and its callers"]
fn find_inputkey_write() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(45));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let controller = client::read_u64(&api, player.addr, 0x2C8);
    let epi = client::read_u64(&api, controller, 0x408);
    assert_ne!(epi, 0, "no EnhancedPlayerInput");

    let w_idx = api.op("string_to_fname", json!({"text": "W"})).result["fname"]
        .as_u64()
        .map(|f| f as u32)
        .expect("no W FName");

    let candidates = keystatemap_w_entries(&api, epi, w_idx);
    if candidates.is_empty() {
        println!("no KeyStateMap-style W entry found; press W once then rerun");
        return;
    }
    println!("W FKeyState candidates (header_off, w_entry):");
    for (h, e) in &candidates {
        println!("  EPI+0x{h:X}  entry 0x{e:X}");
    }

    // FKeyState begins right after the 24-byte FKey. Watch 8 bytes
    // there; InputKey updates the state on a key-down transition.
    let woff: u64 = std::env::var("KS_WOFF")
        .ok()
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0x18);
    let secs = 10u64;

    println!(
        "\n=== TAP W repeatedly for the whole test (~{}s across {} candidates) ===\n\
         watching FKeyState+0x{woff:X} of each W entry\n",
        secs * candidates.len() as u64,
        candidates.len(),
    );

    for (hdr, entry) in &candidates {
        let addr = entry + woff;
        println!("\n########## EPI+0x{hdr:X}: watch W entry 0x{entry:X} + 0x{woff:X} = 0x{addr:X} ##########");
        let res = api
            .op(
                "watch_writes",
                json!({"addr": addr, "len": 8, "duration_ms": secs * 1000}),
            )
            .result;
        let hits = res["hit_count"].as_u64().unwrap_or(0);
        println!("armed={} hit_count={}", res["threads_armed"], hits);
        let Some(records) = res["records"].as_array() else { continue };
        if records.is_empty() {
            println!("  no writes (not KeyStateMap, or wrong FKeyState offset)");
            continue;
        }
        println!("  {} write(s) -- this block IS KeyStateMap; the writer is InputKey:", records.len());
        // One representative record is enough; they repeat.
        let rec = &records[0];
        println!(
            "  InputKey writing instruction: {} = {}+{}",
            rec["rip_after_write"].as_str().unwrap_or("?"),
            rec["rip_module"].as_str().unwrap_or("?"),
            rec["rip_rva"].as_str().unwrap_or("?"),
        );
        if let Some(frames) = rec["stack_return_addrs"].as_array() {
            println!("  callers (nearest first):");
            for f in frames {
                println!(
                    "    {} = {}+{}",
                    f["addr"].as_str().unwrap_or("?"),
                    f["module"].as_str().unwrap_or("?"),
                    f["rva"].as_str().unwrap_or("?"),
                );
            }
        }
    }
    println!(
        "\ndone. The writing instruction's function is InputKey (or the FKeyState \
         setter it calls); walk its entry via CC padding to get its RVA."
    );
}

#[test]
#[ignore = "presses each of W/A/S/D/E through the bot input surface and confirms each registers"]
fn test_bot_all_keys() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(30));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let sel = format!("addr:0x{:X}", player.addr);

    let key = |api: &Api, vk: u32, down: bool| {
        api.op(
            "input.player.commands",
            json!({"commands": [{"kind": "key", "key": vk, "down": down}]}),
        )
    };

    for (name, vk) in [("W", 0x57u32), ("A", 0x41), ("S", 0x53), ("D", 0x44)] {
        let before = actor_location(&api, &sel);
        let r = key(&api, vk, true);
        std::thread::sleep(std::time::Duration::from_millis(600));
        let _ = key(&api, vk, false);
        std::thread::sleep(std::time::Duration::from_millis(400));
        let after = actor_location(&api, &sel);
        let (dx, dy, dz) = (after[0] - before[0], after[1] - before[1], after[2] - before[2]);
        let moved = (dx * dx + dy * dy + dz * dz).sqrt();
        println!("{name} (vk 0x{vk:X}): ok={} moved {moved:.1}", r.ok);
    }

    // E (interact) is a one-shot; just confirm the command path accepts it.
    let e_down = key(&api, 0x45, true);
    std::thread::sleep(std::time::Duration::from_millis(200));
    let e_up = key(&api, 0x45, false);
    println!("E (vk 0x45): down ok={} up ok={}", e_down.ok, e_up.ok);
}

#[test]
#[ignore = "drives the wired bot input surface (input.player.commands) for W and mouse; checks it works"]
fn test_bot_input_wired() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(30));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let sel = format!("addr:0x{:X}", player.addr);

    let before = actor_location(&api, &sel);
    println!("before: {:.1}, {:.1}, {:.1}", before[0], before[1], before[2]);

    // Press W through the bot's registered input surface (VK 0x57 = W).
    let press = api.op(
        "input.player.commands",
        json!({"commands": [{"kind": "key", "key": 0x57, "down": true}]}),
    );
    println!("press W via input.player.commands: ok={} err={:?}", press.ok, press.error);

    for _ in 0..6 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let p = actor_location(&api, &sel);
        let d = {
            let (dx, dy, dz) = (p[0] - before[0], p[1] - before[1], p[2] - before[2]);
            (dx * dx + dy * dy + dz * dz).sqrt()
        };
        println!("  holding W (bot): moved {d:.1}");
    }

    let rel = api.op(
        "input.player.commands",
        json!({"commands": [{"kind": "key", "key": 0x57, "down": false}]}),
    );
    println!("release W: ok={}", rel.ok);
    std::thread::sleep(std::time::Duration::from_millis(300));

    let after_w = actor_location(&api, &sel);
    let (dx, dy, dz) = (after_w[0] - before[0], after_w[1] - before[1], after_w[2] - before[2]);
    let moved = (dx * dx + dy * dy + dz * dz).sqrt();
    println!("walked {moved:.1} via bot key input");

    // Mouse look: a relative mouse delta through the surface.
    let look_before = api.op("input.player.pose", json!({}));
    let yaw_before = look_before.result["pose"]["yaw_deg"].as_f64().unwrap_or(0.0);
    let m = api.op(
        "input.player.commands",
        json!({"commands": [{"kind": "mouse_delta", "dx": 200, "dy": 0}]}),
    );
    std::thread::sleep(std::time::Duration::from_millis(200));
    let look_after = api.op("input.player.pose", json!({}));
    let yaw_after = look_after.result["pose"]["yaw_deg"].as_f64().unwrap_or(0.0);
    println!(
        "mouse dx=200: ok={} yaw {yaw_before:.1} -> {yaw_after:.1} (delta {:.1})",
        m.ok,
        yaw_after - yaw_before
    );

    if moved > 20.0 {
        println!("\n*** SUCCESS: bot input surface walks the character ({moved:.0} units) ***");
    } else {
        println!("\nno walk ({moved:.1}); check the character is on foot and free to move");
    }
}

#[test]
#[ignore = "moves the character via Enhanced Input StartContinuousInputInjectionForAction(ForwardInput)"]
fn test_inject_action_movement() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(30));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let sel = format!("addr:0x{:X}", player.addr);
    let controller = client::read_u64(&api, player.addr, 0x2C8);
    let epi = client::read_u64(&api, controller, 0x408);

    // The Enhanced Input subsystem instance.
    let ss = api.op("walk_class", json!({"class": "EnhancedInputLocalPlayerSubsystem", "max": 1}));
    let subsystem = ss.result["instances"][0]["addr"]
        .as_str()
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .expect("no EnhancedInputLocalPlayerSubsystem");
    let ss_sel = format!("addr:0x{subsystem:X}");
    println!("subsystem: 0x{subsystem:X}");

    // The ForwardInput UInputAction pointer.
    let aid = client::read_u64(&api, epi, 0x598);
    let aid_num = client::read_u64(&api, epi, 0x5A0) as u32 as usize;
    let mut action = 0u64;
    for i in 0..aid_num {
        let ptr = client::read_u64(&api, aid, (i * 0x70) as u64);
        if client::object_name(&api, ptr).as_deref() == Some("ForwardInput") {
            action = ptr;
            break;
        }
    }
    assert_ne!(action, 0, "ForwardInput action not found");
    println!("ForwardInput action: 0x{action:X}\n");

    // Parm block (72 bytes) for StartContinuousInputInjectionForAction:
    //   +0x00 UInputAction* Action
    //   +0x08 FInputActionValue RawValue (FVector Value + ValueType), 32 bytes
    //   +0x28 TArray Modifiers (empty)
    //   +0x38 TArray Triggers (empty)
    // ForwardInput is a 1D axis; Value.X = 1.0, ValueType = Axis1D(1).
    let mut start = [0u8; 72];
    start[0..8].copy_from_slice(&action.to_le_bytes());
    start[8..16].copy_from_slice(&1.0f64.to_le_bytes()); // Value.X
    start[32] = 1; // ValueType at RawValue+0x18 = Axis1D

    let before = actor_location(&api, &sel);
    println!("before: {:.1}, {:.1}, {:.1}", before[0], before[1], before[2]);

    let r = api.call_ufunction(
        "EnhancedInputSubsystemInterface",
        "StartContinuousInputInjectionForAction",
        &ss_sel,
        &start,
    );
    println!("StartContinuousInputInjectionForAction: {:?}", r.map(|_| "ok"));

    for _ in 0..6 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let p = actor_location(&api, &sel);
        let d = {
            let (dx, dy, dz) = (p[0] - before[0], p[1] - before[1], p[2] - before[2]);
            (dx * dx + dy * dy + dz * dz).sqrt()
        };
        println!("  injecting ForwardInput=1.0: moved {d:.1}");
    }

    // Stop: parm is just the Action pointer (8 bytes).
    let mut stop = [0u8; 8];
    stop.copy_from_slice(&action.to_le_bytes());
    let _ = api.call_ufunction(
        "EnhancedInputSubsystemInterface",
        "StopContinuousInputInjectionForAction",
        &ss_sel,
        &stop,
    );
    std::thread::sleep(std::time::Duration::from_millis(300));

    let after = actor_location(&api, &sel);
    let (dx, dy, dz) = (after[0] - before[0], after[1] - before[1], after[2] - before[2]);
    let moved = (dx * dx + dy * dy + dz * dz).sqrt();
    println!("after: {:.1}, {:.1}, {:.1}\ntotal moved: {moved:.1}", after[0], after[1], after[2]);
    if moved > 20.0 {
        println!("\n*** SUCCESS: Enhanced Input injection walked the character {moved:.0} units ***");
    } else {
        println!("\nno movement ({moved:.1}); try ValueType=Boolean(0) or check the character is on foot");
    }
}

#[test]
#[ignore = "finds the Enhanced Input subsystem and InjectInputForAction parm layout"]
fn research_inject_action() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(30));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    // Find the EnhancedInputLocalPlayerSubsystem instance.
    for class in ["EnhancedInputLocalPlayerSubsystem", "EnhancedInputWorldSubsystem"] {
        let r = api.op("walk_class", json!({"class": class, "max": 4}));
        println!("walk_class({class}): ok={} -> {}", r.ok, r.result);
        let r2 = api.op("walk_class_chain", json!({"class": class, "max": 4}));
        println!("walk_class_chain({class}): ok={} -> {}", r2.ok, r2.result);
    }

    // List inject/action functions across the subsystem + interface.
    for class in [
        "EnhancedInputLocalPlayerSubsystem",
        "EnhancedInputSubsystemInterface",
        "EnhancedInputWorldSubsystem",
    ] {
        println!("\n=== {class} functions (inject/action) ===");
        let r = api.op("class_functions_by_name", json!({"class": class}));
        match r.result["functions"].as_array() {
            Some(fns) => {
                let mut any = false;
                for f in fns {
                    let n = f["name"].as_str().unwrap_or("");
                    let l = n.to_ascii_lowercase();
                    if l.contains("inject") || l.contains("action") {
                        any = true;
                        println!(
                            "  {n} ({} parms, {} bytes)",
                            f["num_parms"].as_u64().unwrap_or(0),
                            f["parms_size"].as_u64().unwrap_or(0)
                        );
                        print_function_parameters(&api, class, n);
                    }
                }
                if !any {
                    println!("  ({} total functions, none inject/action)", fns.len());
                }
            }
            None => println!("  lookup failed: {:?}", r.error),
        }
    }
    println!("\n=== FInputActionValue struct ===");
    for s in ["InputActionValue", "FInputActionValue"] {
        let r = api.op("discover_struct_detail", json!({"name": s}));
        println!("{s}: ok={} -> {}", r.ok, r.result);
    }

    // The ForwardInput UInputAction pointer, from ActionInstanceData.
    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let controller = client::read_u64(&api, player.addr, 0x2C8);
    let epi = client::read_u64(&api, controller, 0x408);
    let aid = client::read_u64(&api, epi, 0x598);
    let aid_num = client::read_u64(&api, epi, 0x5A0) as u32 as usize;
    for i in 0..aid_num {
        let ptr = client::read_u64(&api, aid, (i * 0x70) as u64);
        if client::object_name(&api, ptr).as_deref() == Some("ForwardInput") {
            println!("\nForwardInput UInputAction: 0x{ptr:X}");
            break;
        }
    }
}

#[test]
#[ignore = "presses W via InputKey slot 88 and checks the character actually walks"]
fn test_inputkey_movement() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(30));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let sel = format!("addr:0x{:X}", player.addr);
    let controller = client::read_u64(&api, player.addr, 0x2C8);
    let epi = client::read_u64(&api, controller, 0x408);

    let w_ci = api.op("string_to_fname", json!({"text": "W"})).result["fname"]
        .as_u64()
        .map(|f| f as u32)
        .expect("no W FName");

    // Real W FKey with a valid FKeyDetails pointer.
    let fkey: [u8; 24] = keystatemap_w_entries(&api, epi, w_ci)
        .into_iter()
        .find_map(|(_, entry)| {
            let b = client::read_bytes(&api, entry, 0, 24);
            let details = u64::from_le_bytes(b[8..16].try_into().unwrap());
            (b.len() == 24 && details > 0x1_0000_0000).then(|| {
                let mut a = [0u8; 24];
                a.copy_from_slice(&b);
                a
            })
        })
        .expect("no W FKey with valid FKeyDetails");
    let fkey_hex = hex::encode(fkey);
    let slot = std::env::var("IK_SLOT").ok().and_then(|s| s.parse().ok()).unwrap_or(88u64);

    // ForwardInput entry in ActionInstanceData, to see if the action fires.
    let aid = client::read_u64(&api, epi, 0x598);
    let aid_num = client::read_u64(&api, epi, 0x5A0) as u32 as usize;
    let mut fi = 0u64;
    for i in 0..aid_num {
        let ptr = client::read_u64(&api, aid, (i * 0x70) as u64);
        if client::object_name(&api, ptr).as_deref() == Some("ForwardInput") {
            fi = aid + (i * 0x70) as u64;
            break;
        }
    }

    let before = actor_location(&api, &sel);
    println!("before: {:.1}, {:.1}, {:.1}", before[0], before[1], before[2]);

    let press = api.op(
        "try_inputkey",
        json!({"slot": slot, "key_name": "W", "pressed": true, "fkey_hex": fkey_hex}),
    );
    println!("press slot {slot}: ok={} returned={}", press.ok, press.result["returned"]);

    // Hold ~1.5s, re-asserting the down event, and sample position +
    // the ForwardInput action state.
    for _ in 0..6 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let _ = api.op(
            "try_inputkey",
            json!({"slot": slot, "key_name": "W", "pressed": true, "fkey_hex": fkey_hex}),
        );
        let p = actor_location(&api, &sel);
        let d = {
            let (dx, dy, dz) = (p[0] - before[0], p[1] - before[1], p[2] - before[2]);
            (dx * dx + dy * dy + dz * dz).sqrt()
        };
        let (trig, val) = if fi != 0 {
            let t = client::read_bytes(&api, fi, 0x10, 1);
            let v = client::read_bytes(&api, fi, 0x40, 8);
            (
                t.first().copied().unwrap_or(0),
                if v.len() == 8 { f64::from_le_bytes(v.try_into().unwrap()) } else { 0.0 },
            )
        } else {
            (0, 0.0)
        };
        println!("  holding W: moved {d:.1}  ForwardInput trigger={trig} value={val:.2}");
    }

    let _ = api.op(
        "try_inputkey",
        json!({"slot": slot, "key_name": "W", "pressed": false, "fkey_hex": fkey_hex}),
    );
    std::thread::sleep(std::time::Duration::from_millis(300));
    let after = actor_location(&api, &sel);
    let (dx, dy, dz) = (after[0] - before[0], after[1] - before[1], after[2] - before[2]);
    let moved = (dx * dx + dy * dy + dz * dz).sqrt();
    println!("after: {:.1}, {:.1}, {:.1}", after[0], after[1], after[2]);
    println!("total moved: {moved:.1}");
    if moved > 20.0 {
        println!("\n*** SUCCESS: InputKey slot {slot} walked the character {moved:.0} units ***");
    } else {
        println!("\nplayer did not move much ({moved:.1}); check the character is on foot and free to walk");
    }
}

#[test]
#[ignore = "reads W's real FKey (with FKeyDetails) from the InputMappingContext"]
fn dump_w_fkey() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let controller = client::read_u64(&api, player.addr, 0x2C8);

    let mc_off = field_offset(&api, "BP_SGKController_C", "Mapping Context")
        .expect("no Mapping Context field");
    let mc = client::read_u64(&api, controller, mc_off);
    assert_ne!(mc, 0, "null mapping context");
    println!("Mapping Context: 0x{mc:X}");

    // Mappings TArray at +0x30 inside InputMappingContext, stride 0x50,
    // action ptr at entry+0x20, FKey (FName + FKeyDetails) at entry+0x28.
    let (data, num, _max) = {
        let b = client::read_bytes(&api, mc, 0x30, 16);
        (
            u64::from_le_bytes(b[0..8].try_into().unwrap()),
            u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize,
            0,
        )
    };
    println!("Mappings: {num} entries at 0x{data:X}\n");

    let stride = 0x50usize;
    for i in 0..num {
        let entry = data + (i * stride) as u64;
        let action_ptr = client::read_u64(&api, entry, 0x20);
        let action = client::object_name(&api, action_ptr).unwrap_or_default();
        if action != "ForwardInput" {
            continue;
        }
        let fkey = client::read_bytes(&api, entry, 0x28, 24);
        if fkey.len() < 24 {
            continue;
        }
        let ci = u32::from_le_bytes(fkey[0..4].try_into().unwrap());
        let num_f = u32::from_le_bytes(fkey[4..8].try_into().unwrap());
        let details = u64::from_le_bytes(fkey[8..16].try_into().unwrap());
        let details_ref = u64::from_le_bytes(fkey[16..24].try_into().unwrap());
        let key_name = client::fname_to_string(&api, ci as u64).unwrap_or_default();
        println!(
            "ForwardInput mapping entry 0x{entry:X}: key={key_name} FName(ci={ci},num={num_f}) \
             details=0x{details:X} details_ref=0x{details_ref:X}"
        );
        let valid = details > 0x1_0000_0000;
        println!("  FKeyDetails pointer is {}", if valid { "VALID (usable)" } else { "null/invalid" });
    }
}

#[test]
#[ignore = "finds InputKey's vtable slot: the one whose call makes IsInputKeyDown(W) return true"]
fn find_inputkey_slot_via_iskeydown() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(30));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let controller = client::read_u64(&api, player.addr, 0x2C8);
    assert_ne!(controller, 0, "no controller");
    let sel = format!("addr:0x{controller:X}");

    let epi = client::read_u64(&api, controller, 0x408);
    let w = api.op("string_to_fname", json!({"text": "W"})).result["fname"]
        .as_u64()
        .expect("no W FName");
    let w_ci = (w & 0xFFFF_FFFF) as u32;

    // Lift a real W FKey (FName + valid FKeyDetails pointer) from an EPI
    // block that carries one; native input code derefs the details, so
    // a name-only FKey crashes it.
    let fkey: [u8; 24] = keystatemap_w_entries(&api, epi, w_ci)
        .into_iter()
        .find_map(|(_, entry)| {
            let b = client::read_bytes(&api, entry, 0, 24);
            let details = u64::from_le_bytes(b[8..16].try_into().unwrap());
            (b.len() == 24 && details > 0x1_0000_0000).then(|| {
                let mut a = [0u8; 24];
                a.copy_from_slice(&b);
                a
            })
        })
        .expect("no W FKey with valid FKeyDetails found on the EPI object");
    let fkey_hex = hex::encode(fkey);
    println!(
        "using real W FKey: details=0x{:X}\n",
        u64::from_le_bytes(fkey[8..16].try_into().unwrap())
    );

    // Parm block for APlayerController::IsInputKeyDown(FKey Key) -> bool:
    // FKey (24 bytes, real) at +0x00, ReturnValue bool at +0x18.
    let make_parms = || -> [u8; 0x20] {
        let mut p = [0u8; 0x20];
        p[0..24].copy_from_slice(&fkey);
        p
    };
    let is_down = |api: &Api| -> bool {
        match api.call_ufunction("PlayerController", "IsInputKeyDown", &sel, &make_parms()) {
            Ok((out, _)) => out.get(0x18).copied().unwrap_or(0) != 0,
            Err(e) => {
                println!("  IsInputKeyDown failed: {e}");
                false
            }
        }
    };

    println!("baseline IsInputKeyDown(W) = {}\n", is_down(&api));

    // EnhancedInput override slots from the vtable dump; 91 and 93 hang
    // or crash when called with FInputKeyParams, so skip them.
    let slots = [87usize, 88, 89, 90, 92, 94, 95, 96, 97, 98, 99];
    for slot in slots {
        let press = api.op(
            "try_inputkey",
            json!({"slot": slot, "key_name": "W", "pressed": true, "fkey_hex": fkey_hex}),
        );
        if !press.ok {
            println!("slot {slot}: press failed: {:?}", press.error);
            continue;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        let down = is_down(&api);
        let _ = api.op(
            "try_inputkey",
            json!({"slot": slot, "key_name": "W", "pressed": false, "fkey_hex": fkey_hex}),
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
        let after = is_down(&api);
        println!("slot {slot}: after press IsInputKeyDown(W)={down}, after release={after}");
        if down && !after {
            println!("\n*** slot {slot} IS InputKey: it set W down in KeyStateMap and release cleared it ***");
            let _ = api.op(
                "try_inputkey",
                json!({"slot": slot, "key_name": "W", "pressed": false, "fkey_hex": fkey_hex}),
            );
            return;
        }
    }
    println!("\nno slot toggled IsInputKeyDown(W). If none worked, InputKey may need a valid FKeyDetails.");
}

#[test]
#[ignore = "calls each EnhancedInput vtable slot as InputKey(W) and checks if the ForwardInput action fires"]
fn verify_inputkey_via_action() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(30));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let controller = client::read_u64(&api, player.addr, 0x2C8);
    let epi = client::read_u64(&api, controller, 0x408);
    assert_ne!(epi, 0, "no EnhancedPlayerInput");

    // Find the ForwardInput entry in ActionInstanceData (+0x598, stride
    // 0x70). Its trigger byte (+0x10) and value (+0x40) change when a
    // real W press propagates through the input pipeline.
    let aid_data = client::read_u64(&api, epi, 0x598);
    let aid_num = client::read_u64(&api, epi, 0x5A0) as u32 as usize;
    let stride: usize = 0x70;
    let mut fi_entry = 0u64;
    for i in 0..aid_num {
        let ptr = client::read_u64(&api, aid_data, (i * stride) as u64);
        if client::object_name(&api, ptr).as_deref() == Some("ForwardInput") {
            fi_entry = aid_data + (i * stride) as u64;
            break;
        }
    }
    assert_ne!(fi_entry, 0, "ForwardInput not found in ActionInstanceData");
    println!("ForwardInput entry: 0x{fi_entry:X}\n");

    let read_trigger = |api: &Api| -> (u8, f64) {
        let t = client::read_bytes(api, fi_entry, 0x10, 1);
        let v = client::read_bytes(api, fi_entry, 0x40, 8);
        let tb = t.first().copied().unwrap_or(0);
        let vf = if v.len() == 8 {
            f64::from_le_bytes(v.try_into().unwrap())
        } else {
            0.0
        };
        (tb, vf)
    };

    // EnhancedInput override slots from the vtable dump, minus 93 which
    // crashes when called with FInputKeyParams (wrong-shaped callee).
    let slots = [87usize, 88, 89, 90, 91, 92, 94, 95, 96, 97, 98, 99];
    for slot in slots {
        let press = api.op("try_inputkey", json!({"slot": slot, "key_name": "W", "pressed": true}));
        if !press.ok {
            println!("slot {slot}: press failed: {:?}", press.error);
            continue;
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
        let (tb, vf) = read_trigger(&api);
        let _ = api.op("try_inputkey", json!({"slot": slot, "key_name": "W", "pressed": false}));
        std::thread::sleep(std::time::Duration::from_millis(200));
        let (tb_after, vf_after) = read_trigger(&api);
        println!(
            "slot {slot}: returned={} -> ForwardInput trigger={tb} value={vf:.2} (after release: trigger={tb_after} value={vf_after:.2})",
            press.result["returned"]
        );
        if tb == 2 || vf > 0.5 {
            println!("\n*** slot {slot} IS InputKey: calling it fired the ForwardInput action ***");
            let _ = api.op("try_inputkey", json!({"slot": slot, "key_name": "W", "pressed": false}));
            return;
        }
    }
    println!("\nno slot fired ForwardInput. If none worked, the game may be paused/at a menu, \
              or InputKey needs a valid FKeyDetails.");
}

#[test]
#[ignore = "dumps EnhancedPlayerInput vtable slots as RVAs and flags the input-path function"]
fn dump_epi_vtable_rvas() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let controller = client::read_u64(&api, player.addr, 0x2C8);
    let epi = client::read_u64(&api, controller, 0x408);
    assert_ne!(epi, 0, "no EnhancedPlayerInput");

    let probe = api.op("watch_writes", json!({"addr": epi, "len": 8, "duration_ms": 1}));
    let base = u64::from_str_radix(
        probe.result["exe_base"].as_str().unwrap().trim_start_matches("0x"),
        16,
    )
    .unwrap();
    let exe_end = base
        + u64::from_str_radix(
            probe.result["exe_size"].as_str().unwrap().trim_start_matches("0x"),
            16,
        )
        .unwrap();

    let vtable = client::read_u64(&api, epi, 0);
    println!("EPI 0x{epi:X} vtable 0x{vtable:X} exe base 0x{base:X}\n");

    // The this=EPI input-path function the write watchpoint captured.
    let target_rva: u64 = 0x3cb9360;

    for slot in 0u64..120 {
        let fn_addr = client::read_u64(&api, vtable, slot * 8);
        if fn_addr == 0 {
            println!("slot {slot}: null (vtable end)");
            break;
        }
        let in_exe = fn_addr >= base && fn_addr < exe_end;
        let rva = if in_exe { fn_addr - base } else { 0 };
        let mark = if in_exe && rva == target_rva {
            "  <== this=EPI input-path fn (+0x3cb9360)"
        } else {
            ""
        };
        let where_ = if in_exe {
            format!("+0x{rva:x}")
        } else {
            format!("0x{fn_addr:X} (plugin/other module)")
        };
        println!("slot {slot:3}: {where_}{mark}");
    }
}

#[test]
#[ignore = "sweeps EnhancedPlayerInput vtable slots calling InputKey(W); the slot that moves the player is InputKey"]
fn sweep_inputkey_slots() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(30));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let sel = format!("addr:0x{:X}", player.addr);

    // bl-sdk calls InputKey at EnhancedPlayerInput vtable index ~85;
    // MISERY's EnhancedInput plugin virtuals sit at slots 87-99.
    let start: usize = std::env::var("SLOT_START").ok().and_then(|s| s.parse().ok()).unwrap_or(85);
    let end: usize = std::env::var("SLOT_END").ok().and_then(|s| s.parse().ok()).unwrap_or(100);

    println!("sweeping EnhancedPlayerInput vtable slots {start}..{end} calling InputKey(W)\n");

    for slot in start..end {
        let before = actor_location(&api, &sel);
        let press = api.op("try_inputkey", json!({"slot": slot, "key_name": "W", "pressed": true}));
        if !press.ok {
            println!("slot {slot}: press failed: {:?}", press.error);
            continue;
        }
        std::thread::sleep(std::time::Duration::from_millis(700));
        let _rel = api.op("try_inputkey", json!({"slot": slot, "key_name": "W", "pressed": false}));
        std::thread::sleep(std::time::Duration::from_millis(200));
        let after = actor_location(&api, &sel);
        let dx = after[0] - before[0];
        let dy = after[1] - before[1];
        let dz = after[2] - before[2];
        let moved = (dx * dx + dy * dy + dz * dz).sqrt();
        println!(
            "slot {slot}: returned={} moved={moved:.2}",
            press.result["returned"]
        );
        if moved > 5.0 {
            println!("\n*** slot {slot} IS InputKey: calling it with W moved the player {moved:.1} units ***");
            // Make sure the key is released.
            let _ = api.op("try_inputkey", json!({"slot": slot, "key_name": "W", "pressed": false}));
            return;
        }
    }
    println!("\nno slot in {start}..{end} moved the player. Widen the range, or the \
              player cannot move right now (menu / not on ground).");
}

#[test]
#[ignore = "finds InputKey by which chain frame receives W's FKey as a parameter"]
fn find_inputkey_by_param() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(120));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let controller = client::read_u64(&api, player.addr, 0x2C8);
    let epi = client::read_u64(&api, controller, 0x408);

    let w_idx = api.op("string_to_fname", json!({"text": "W"})).result["fname"]
        .as_u64()
        .map(|f| f as u32)
        .expect("no W FName");

    // W's FKeyDetails pointer from the KeyStateMap W entry: a second,
    // stronger signal (FKey = FName@0x00 + FKeyDetails@0x08).
    let w_details = keystatemap_w_entries(&api, epi, w_idx)
        .into_iter()
        .find_map(|(_, entry)| {
            let d = client::read_u64(&api, entry, 0x08);
            (d > 0x1_0000_0000).then_some(d)
        })
        .unwrap_or(0);
    println!("W FName index = 0x{w_idx:X}, W FKeyDetails ptr = 0x{w_details:X}\n");

    let probe = api.op("watch_writes", json!({"addr": epi, "len": 8, "duration_ms": 1}));
    let base = u64::from_str_radix(
        probe.result["exe_base"].as_str().unwrap().trim_start_matches("0x"),
        16,
    )
    .unwrap();

    // Every input-thread function on the KeyStateMap write chain
    // (hot utilities and the navmesh red herring excluded).
    let frames: &[u64] = &[
        0x11aee60, 0x3cb9360, 0x39f1210, 0x10d4790, 0x38b2af0, 0x39ee780, 0x3ce6d90, 0x12cc170,
        0xf89d50,
    ];

    let secs = 8u64;
    println!(
        "=== TAP W steadily for the whole test (~{}s across {} frames) ===\n",
        secs * frames.len() as u64,
        frames.len(),
    );

    let heap_lo = 0x1_0000_0000u64;
    let heap_hi = 0x8000_0000_0000u64;

    for &rva in frames {
        let res = api
            .op(
                "watch_writes",
                json!({"addr": base + rva, "mode": "exec", "len": 1, "duration_ms": secs * 1000}),
            )
            .result;
        let Some(rec) = res["records"].as_array().and_then(|r| r.first()) else {
            println!("+0x{rva:<8x} did not run");
            continue;
        };
        let reg = |n: &str| {
            u64::from_str_radix(rec[n].as_str().unwrap_or("0x0").trim_start_matches("0x"), 16)
                .unwrap_or(0)
        };
        let mut found = false;
        for n in ["rcx", "rdx", "r8", "r9"] {
            let v = reg(n);
            if v < heap_lo || v >= heap_hi {
                continue;
            }
            // One guarded read; the guard makes a bad pointer safe.
            let buf = client::read_bytes(&api, v, 0, 0x40);
            let mut o = 0usize;
            while o + 4 <= buf.len() {
                let iv = u32::from_le_bytes(buf[o..o + 4].try_into().unwrap());
                if iv == w_idx {
                    let d = if o + 12 <= buf.len() {
                        u64::from_le_bytes(buf[o + 4..o + 12].try_into().unwrap())
                    } else {
                        0
                    };
                    let strong = w_details != 0 && d == w_details;
                    println!(
                        "*** +0x{rva:x}: W FName in [{n}]+0x{o:X}{} -> this frame receives W's FKey (InputKey)",
                        if strong { " with matching FKeyDetails" } else { "" }
                    );
                    found = true;
                }
                o += 4;
            }
        }
        if !found {
            println!(
                "+0x{rva:<8x} ran; args carry no W FKey (rcx=0x{:X} rdx=0x{:X} r8=0x{:X} r9=0x{:X})",
                reg("rcx"), reg("rdx"), reg("r8"), reg("r9")
            );
        }
    }
    println!("\ndone. The frame that receives W's FKey is InputKey.");
}

#[test]
#[ignore = "proves the read_bytes op rejects bad pointers instead of crashing the game"]
fn read_op_guards_bad_pointer() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    // A valid read still works: the live player object.
    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let good = api.op(
        "read_bytes",
        json!({"instance_selector": format!("addr:0x{:X}", player.addr), "length": 16}),
    );
    assert!(good.ok, "valid read failed: {:?}", good.error);
    println!("valid read of player object: ok, {} bytes", 16);

    // Bad pointers that would have faulted the copy before the guard:
    // a near-null address and high addresses no user allocation reaches.
    let bad_addrs: [u64; 3] = [0x1, 0xDEAD_0000_0000, 0x7FFF_FFFF_0000];
    for a in bad_addrs {
        let r = api.op(
            "read_bytes",
            json!({"instance_selector": format!("addr:0x{a:X}"), "length": 64}),
        );
        println!("read 0x{a:X}: ok={} error={:?}", r.ok, r.error);
        assert!(!r.ok, "read of bad address 0x{a:X} should be rejected, not succeed");
    }

    // write_bytes must reject the same bad pointers (an unmapped or
    // read-only write would fault the game too).
    for a in bad_addrs {
        let r = api.op(
            "write_bytes",
            json!({"instance_selector": format!("addr:0x{a:X}"), "bytes_hex": "00"}),
        );
        println!("write 0x{a:X}: ok={} error={:?}", r.ok, r.error);
        assert!(!r.ok, "write to bad address 0x{a:X} should be rejected, not succeed");
    }

    // The decisive proof: the game and its control plane are still
    // alive after the bad reads and writes (before the guards, they
    // crashed it). Re-read the valid player object; a response means
    // the process and its listener survived.
    let after = api.op(
        "read_bytes",
        json!({"instance_selector": format!("addr:0x{:X}", player.addr), "length": 16}),
    );
    assert!(after.ok, "control plane died after bad reads: {:?}", after.error);
    println!("\nSUCCESS: bad pointers rejected, game still alive (valid read still works)");
}

#[test]
#[ignore = "empirically locates FInputKeyParams by finding W's FName index in the arg registers' memory"]
fn decode_inputkeyparams() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(45));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let controller = client::read_u64(&api, player.addr, 0x2C8);
    let epi = client::read_u64(&api, controller, 0x408);
    println!("controller: 0x{controller:X}  EPI: 0x{epi:X}");

    let w_idx = api.op("string_to_fname", json!({"text": "W"})).result["fname"]
        .as_u64()
        .map(|f| f as u32)
        .expect("no W FName");
    println!("W FName comparison_index = {w_idx} (0x{w_idx:X})\n");

    // exe base.
    let probe = api.op("watch_writes", json!({"addr": epi, "len": 8, "duration_ms": 1}));
    let base = u64::from_str_radix(
        probe.result["exe_base"].as_str().unwrap().trim_start_matches("0x"),
        16,
    )
    .unwrap();

    // Target: APlayerController::InputKey by default.
    let rva = std::env::var("IK_RVA")
        .ok()
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0x39f1210);
    let addr = base + rva;

    let secs = 10u64;
    println!("=== TAP W steadily for ~{secs}s (exec on +0x{rva:X}) ===\n");
    let res = api
        .op(
            "watch_writes",
            json!({"addr": addr, "mode": "exec", "len": 1, "duration_ms": secs * 1000}),
        )
        .result;
    let Some(rec) = res["records"].as_array().and_then(|r| r.first()) else {
        println!("function did not run; press W during the window");
        return;
    };

    let reg = |name: &str| -> u64 {
        u64::from_str_radix(rec[name].as_str().unwrap_or("0x0").trim_start_matches("0x"), 16)
            .unwrap_or(0)
    };
    let regs = [("rcx", reg("rcx")), ("rdx", reg("rdx")), ("r8", reg("r8")), ("r9", reg("r9"))];
    for (n, v) in &regs {
        println!("  {n} = 0x{v:X}");
    }
    println!();

    let heap_lo = 0x1_0000_0000u64;
    let heap_hi = 0x8000_0000_0000u64;

    // Read one region (a single control-plane call) and scan the local
    // buffer for W's FName index. NO pointer chasing: recursive reads
    // hang the control plane, and the region either holds W or it does
    // not. Returns the pointer-sized values seen, for one manual step
    // deeper if needed.
    let scan = |label: &str, at: u64| -> Vec<u64> {
        if at < heap_lo || at >= heap_hi {
            println!("{label}: 0x{at:X} not a heap address");
            return Vec::new();
        }
        let buf = client::read_bytes(&api, at, 0, 0x80);
        if buf.is_empty() {
            println!("{label}: 0x{at:X} unreadable");
            return Vec::new();
        }
        println!("{label} at 0x{at:X}:");
        for row in 0..(buf.len() / 16) {
            let o = row * 16;
            let hex: Vec<String> = buf[o..o + 16].iter().map(|x| format!("{x:02X}")).collect();
            println!("  +0x{o:02X}  {}", hex.join(" "));
        }
        let mut off = 0usize;
        while off + 4 <= buf.len() {
            let v = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap());
            if v == w_idx {
                println!("  *** W FName index at {label}+0x{off:X} (abs 0x{:X}) ***", at + off as u64);
            }
            off += 4;
        }
        let mut ptrs = Vec::new();
        let mut o = 0usize;
        while o + 8 <= buf.len() {
            ptrs.push(u64::from_le_bytes(buf[o..o + 8].try_into().unwrap()));
            o += 8;
        }
        ptrs
    };

    // rdx is the presumed parameter pointer. Dump it and its distinct
    // heap-pointer fields, one level, each a single bounded read.
    let rdx_ptrs = scan("rdx", reg("rdx"));
    println!();
    let mut seen: Vec<u64> = Vec::new();
    for p in rdx_ptrs {
        if p >= heap_lo && p < heap_hi && !seen.contains(&p) {
            seen.push(p);
            if seen.len() > 8 {
                break;
            }
            scan(&format!("  rdx->*(0x{p:X})"), p);
        }
    }
    println!();
    scan("r9", reg("r9"));
    println!("\ndone.");
}

#[test]
#[ignore = "names InputKey by its `this` register and dumps the real FInputKeyParams"]
fn name_inputkey_by_rcx() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(60));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let controller = client::read_u64(&api, player.addr, 0x2C8);
    let epi = client::read_u64(&api, controller, 0x408);
    assert_ne!(epi, 0, "no EnhancedPlayerInput");
    println!("controller: 0x{controller:X}");
    println!("EnhancedPlayerInput: 0x{epi:X}");

    // exe base from a 1 ms throwaway watch.
    let probe = api.op("watch_writes", json!({"addr": epi, "len": 8, "duration_ms": 1}));
    let base = u64::from_str_radix(
        probe.result["exe_base"].as_str().expect("no exe_base").trim_start_matches("0x"),
        16,
    )
    .expect("bad exe_base");

    // The four input-thread functions in the KeyStateMap write chain,
    // innermost (store) to outermost (entry).
    let candidates: &[(&str, u64)] = &[
        ("inner-store", 0x11aee60),
        ("mid-a", 0x3cb9360),
        ("mid-b", 0x39f1210),
        ("outer-entry", 0x3213800),
    ];

    let secs = 10u64;
    println!(
        "\n=== TAP W steadily for ~{}s (arming exec on each candidate, reading `this`) ===\n",
        secs * candidates.len() as u64,
    );

    for (label, rva) in candidates {
        let addr = base + rva;
        let res = api
            .op(
                "watch_writes",
                json!({"addr": addr, "mode": "exec", "len": 1, "duration_ms": secs * 1000}),
            )
            .result;
        let Some(rec) = res["records"].as_array().and_then(|r| r.first()) else {
            println!("{label:12} +0x{rva:<8x} did not run");
            continue;
        };
        let rcx = u64::from_str_radix(
            rec["rcx"].as_str().unwrap_or("0x0").trim_start_matches("0x"),
            16,
        )
        .unwrap_or(0);
        let rdx = u64::from_str_radix(
            rec["rdx"].as_str().unwrap_or("0x0").trim_start_matches("0x"),
            16,
        )
        .unwrap_or(0);
        let this = if rcx == controller {
            "this=PlayerController -> APlayerController::InputKey".to_string()
        } else if rcx == epi {
            "this=EnhancedPlayerInput -> UPlayerInput::InputKey".to_string()
        } else {
            let name = client::object_name(&api, rcx).unwrap_or_else(|| "<unknown>".into());
            format!("this=0x{rcx:X} ({name})")
        };
        println!("{label:12} +0x{rva:<8x} rcx=0x{rcx:X} rdx=0x{rdx:X}  {this}");

        // If this is InputKey (this = controller or EPI), rdx is the
        // FInputKeyParams pointer. Dump it: this is the real, byte-exact
        // parameter block earlier attempts had to guess.
        if (rcx == controller || rcx == epi) && rdx > 0x1_0000_0000 {
            let params = client::read_bytes(&api, rdx, 0, 0x40);
            println!("  FInputKeyParams at 0x{rdx:X} ({} bytes):", params.len());
            for row in 0..(params.len() / 16) {
                let o = row * 16;
                let hex: Vec<String> =
                    params[o..o + 16].iter().map(|x| format!("{x:02X}")).collect();
                println!("    +0x{o:02X}  {}", hex.join(" "));
            }
            // FKey FName is the first 4 bytes; resolve it.
            if params.len() >= 4 {
                let idx = u32::from_le_bytes(params[0..4].try_into().unwrap());
                let name = client::fname_to_string(&api, idx as u64)
                    .unwrap_or_else(|| format!("index {idx}"));
                println!("    FKey FName = {name}");
            }
        }
    }
    println!("\ndone.");
}

#[test]
#[ignore = "decodes KeyStateMap on EnhancedPlayerInput to find the W FKey entry address"]
fn decode_keystatemap_find_w() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(30));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player").expect("no live player");
    let controller = client::read_u64(&api, player.addr, 0x2C8);
    let epi = client::read_u64(&api, controller, 0x408);
    assert_ne!(epi, 0, "no EnhancedPlayerInput");
    println!("EnhancedPlayerInput: 0x{epi:X}");

    // FName comparison index (low 32 bits of the FName u64) for the
    // keys we expect to find in KeyStateMap once they have been
    // pressed. Resolve through the engine's own constructor.
    let key_index = |name: &str| -> Option<u32> {
        let r = api.op("string_to_fname", json!({"text": name}));
        r.result["fname"].as_u64().map(|f| f as u32)
    };
    let keys = ["W", "A", "S", "D", "E", "LeftShift", "SpaceBar"];
    let mut want: Vec<(&str, u32)> = Vec::new();
    for k in keys {
        if let Some(idx) = key_index(k) {
            println!("  FName {k:10} comparison_index = {idx} (0x{idx:X})");
            want.push((k, idx));
        }
    }
    let w_idx = want
        .iter()
        .find(|(n, _)| *n == "W")
        .map(|(_, i)| *i)
        .expect("could not resolve FName for W");
    println!("\nlooking for W (index {w_idx}) in a TSet-shaped block on the EPI object\n");

    // Scan the object for TSparseArray/TSet-shaped headers: a heap
    // pointer, an element count, and a capacity that is >= count.
    // KeyStateMap's element array is one of these; its entries begin
    // with an FKey (FName at +0x00), so a matching key index appears
    // inside the block the header points at.
    let obj = client::read_bytes(&api, epi, 0x28, 0x800);
    let heap_lo = 0x1_0000_0000u64;
    let heap_hi = 0x8000_0000_0000u64;

    for off in (0..obj.len().saturating_sub(16)).step_by(8) {
        let ptr = u64::from_le_bytes(obj[off..off + 8].try_into().unwrap());
        let count = u32::from_le_bytes(obj[off + 8..off + 12].try_into().unwrap());
        let cap = u32::from_le_bytes(obj[off + 12..off + 16].try_into().unwrap());
        if ptr < heap_lo || ptr >= heap_hi || count == 0 || count > 1000 || cap < count || cap > 8192
        {
            continue;
        }
        let hdr_off = 0x28 + off;

        // Read the element block and scan every 4 bytes for any of the
        // key indices. A block holding several of our keys IS the
        // KeyStateMap.
        let block_len = ((cap as u64) * 0x80).min(0x8000);
        let block = client::read_bytes(&api, ptr, 0, block_len);
        if block.is_empty() {
            continue;
        }

        let mut found: Vec<(&str, usize)> = Vec::new();
        let mut b = 0usize;
        while b + 4 <= block.len() {
            let v = u32::from_le_bytes(block[b..b + 4].try_into().unwrap());
            if let Some((name, _)) = want.iter().find(|(_, idx)| *idx == v) {
                found.push((name, b));
            }
            b += 4;
        }
        if found.is_empty() {
            continue;
        }

        println!(
            "CANDIDATE header at EPI+0x{hdr_off:X}: ptr=0x{ptr:X} count={count} cap={cap}"
        );
        for (name, boff) in &found {
            println!(
                "  key {name:10} at block+0x{boff:X}  ->  entry FName addr 0x{:X}",
                ptr + *boff as u64
            );
        }

        // Infer the entry stride from the spacing between two distinct
        // keys, then dump the W entry so we can pick a byte to watch.
        if let Some((_, w_boff)) = found.iter().find(|(n, _)| *n == "W") {
            let w_addr = ptr + *w_boff as u64;
            println!("\n  W entry (FName at 0x{w_addr:X}), first 0x60 bytes:");
            let entry = client::read_bytes(&api, w_addr, 0, 0x60);
            for row in 0..(entry.len() / 16) {
                let o = row * 16;
                let hex: Vec<String> =
                    entry[o..o + 16].iter().map(|x| format!("{x:02X}")).collect();
                println!("    +0x{o:02X}  {}", hex.join(" "));
            }
        }
        println!();
    }

    println!(
        "done. The W FKeyState follows its FKey (FName + 24-byte FKey, then the \
         FKeyState). Pick a byte in the FKeyState that changes on press and \
         write-watch it to catch InputKey."
    );
}

#[test]
#[ignore = "arms an exec breakpoint on the InputKey candidate; confirms it runs on a physical W press"]
fn verify_inputkey_candidate_executes() {
    let Some(api) = api_or_skip() else { return };
    let api = api.with_timeout(std::time::Duration::from_secs(45));
    assert!(offsets_live(&api), "MISERY offsets are not live");

    // WATCH_EXEC_RVAS sweeps a comma-separated list of exe-relative
    // function-entry offsets (hex), arming an exec breakpoint on each
    // in turn. The one that fires only on the input thread and only on
    // a key press (a few hits, one tid) is InputKey; hot utilities
    // fire many times across many tids.
    if let Ok(list) = std::env::var("WATCH_EXEC_RVAS") {
        let rvas: Vec<u64> = list
            .split(',')
            .filter_map(|t| u64::from_str_radix(t.trim().trim_start_matches("0x"), 16).ok())
            .collect();
        // exe base from a 1 ms throwaway watch on the live EPI.
        let player = client::resolve_selector(&api, "live_player").expect("no live player");
        let controller = client::read_u64(&api, player.addr, 0x2C8);
        let epi = client::read_u64(&api, controller, 0x408);
        let probe = api.op("watch_writes", json!({"addr": epi, "len": 8, "duration_ms": 1}));
        let base = u64::from_str_radix(
            probe.result["exe_base"].as_str().expect("no exe_base").trim_start_matches("0x"),
            16,
        )
        .expect("bad exe_base");

        let secs = 8u64;
        println!(
            "\n=== TAP W steadily for the whole test (~{}s across {} functions) ===\n",
            secs * rvas.len() as u64,
            rvas.len(),
        );
        for rva in rvas {
            let addr = base + rva;
            let res = api
                .op(
                    "watch_writes",
                    json!({"addr": addr, "mode": "exec", "len": 1, "duration_ms": secs * 1000}),
                )
                .result;
            let hits = res["hit_count"].as_u64().unwrap_or(0);
            // Count distinct threads that hit.
            let mut tids = std::collections::BTreeSet::new();
            if let Some(recs) = res["records"].as_array() {
                for r in recs {
                    if let Some(t) = r["tid"].as_u64() {
                        tids.insert(t);
                    }
                }
            }
            let verdict = if hits == 0 {
                "never runs"
            } else if tids.len() == 1 {
                "SINGLE THREAD (InputKey-like)"
            } else {
                "many threads (hot utility)"
            };
            println!("  fn +0x{rva:<8x} hits={hits:<3} distinct_tids={} -> {verdict}", tids.len());
        }
        println!("\ndone. The single-thread, few-hits function is InputKey.");
        return;
    }

    // WATCH_EXEC_RVA overrides the target with an exe-relative offset
    // (hex), so a known-good function can be used as a positive
    // control. Default: the prologue-scan InputKey candidate.
    let candidate = if let Ok(rva_s) = std::env::var("WATCH_EXEC_RVA") {
        let rva = u64::from_str_radix(rva_s.trim_start_matches("0x"), 16).expect("bad WATCH_EXEC_RVA");
        // Fetch base from a 1 ms throwaway watch on the live EPI.
        let player = client::resolve_selector(&api, "live_player").expect("no live player");
        let controller = client::read_u64(&api, player.addr, 0x2C8);
        let epi = client::read_u64(&api, controller, 0x408);
        let probe = api.op("watch_writes", json!({"addr": epi, "len": 8, "duration_ms": 1}));
        let base = u64::from_str_radix(
            probe.result["exe_base"].as_str().expect("no exe_base").trim_start_matches("0x"),
            16,
        )
        .expect("bad exe_base");
        let addr = base + rva;
        println!("exec target (override): +0x{rva:x} = 0x{addr:X}");
        addr
    } else {
        let r = api.op("find_inputkey", json!({}));
        let c = r.result["prologue_scan"]["hits"]
            .as_array()
            .and_then(|h| h.first())
            .and_then(|h| h["fn_addr"].as_str())
            .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .expect("no prologue-scan InputKey candidate");
        println!("InputKey candidate: 0x{c:X}");
        c
    };

    let secs = 15u64;
    println!(
        "\n=== TAP W a few times over the next {secs} seconds ===\n\
         (arming an EXECUTE breakpoint on the candidate; if it runs on a W\n\
         press, it is on the real input path and we capture its callers)\n"
    );

    let res = api
        .op(
            "watch_writes",
            json!({"addr": candidate, "len": 1, "mode": "exec", "duration_ms": secs * 1000}),
        )
        .result;

    println!(
        "mode={} threads armed={} hit_count={} arm_diag={}",
        res["mode"], res["threads_armed"], res["hit_count"], res["arm_diag"],
    );

    let Some(records) = res["records"].as_array() else {
        println!("no records array");
        return;
    };
    if records.is_empty() {
        println!(
            "\nCANDIDATE DID NOT RUN on a W press. It is NOT InputKey (or W was \
             not pressed). The prologue scan found the wrong function."
        );
        return;
    }

    println!(
        "\nCANDIDATE RAN {} time(s) on physical input. It IS on the input path.",
        records.len()
    );
    for (i, rec) in records.iter().enumerate() {
        println!("\n--- execution #{i} (tid {}) ---", rec["tid"]);
        println!(
            "  entry hit: {} = {}+{}",
            rec["rip_after_write"].as_str().unwrap_or("?"),
            rec["rip_module"].as_str().unwrap_or("?"),
            rec["rip_rva"].as_str().unwrap_or("?"),
        );
        if let Some(frames) = rec["stack_return_addrs"].as_array() {
            println!("  callers (return addresses, nearest first):");
            for f in frames {
                println!(
                    "    {} = {}+{}",
                    f["addr"].as_str().unwrap_or("?"),
                    f["module"].as_str().unwrap_or("?"),
                    f["rva"].as_str().unwrap_or("?"),
                );
            }
        }
    }
}

#[test]
#[ignore = "dump raw bytes from vtable functions to analyze InputKey"]
fn dump_inputkey_fn_bytes() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    // Dump controller vtable slots that might be InputKey.
    // APlayerController::InputKey is virtual on the controller.
    // We need to find which slot it is. Dump a range of slots
    // and look for functions that take 2 params (this + FInputKeyParams*).
    // Dump controller slot 130 (thunk target from slot 100) in detail
    println!("=== CONTROLLER slot 130 (512 bytes) ===");
    let r = api.op("dump_fn_bytes", json!({"target": "controller", "slot": 130, "count": 512}));
    if let Some(lines) = r.result["hex"].as_array() {
        for line in lines {
            println!("{}", line.as_str().unwrap_or(""));
        }
    }

    // Also dump controller slots 97 and 98 (both check rdx null)
    println!("\n=== CONTROLLER slot 97 (256 bytes) ===");
    let r = api.op("dump_fn_bytes", json!({"target": "controller", "slot": 97, "count": 256}));
    if let Some(lines) = r.result["hex"].as_array() {
        for line in lines {
            println!("{}", line.as_str().unwrap_or(""));
        }
    }

    println!("\n=== CONTROLLER slot 98 (256 bytes) ===");
    let r = api.op("dump_fn_bytes", json!({"target": "controller", "slot": 98, "count": 256}));
    if let Some(lines) = r.result["hex"].as_array() {
        for line in lines {
            println!("{}", line.as_str().unwrap_or(""));
        }
    }

    println!("=== CONTROLLER vtable (first 16 bytes per slot, slots 125-200) ===");
    for slot in 125u64..200 {
        let r = api.op("dump_fn_bytes", json!({
            "target": "controller",
            "slot": slot,
            "count": 32,
        }));
        if r.ok {
            let addr = r.result["fn_addr"].as_str().unwrap_or("?");
            if let Some(lines) = r.result["hex"].as_array() {
                let first = lines[0].as_str().unwrap_or("");
                println!("[{slot:3}] {addr}: {first}");
            }
        }
    }

    // Also dump EnhancedPlayerInput slot 94 for reference
    println!("\n=== PLAYERINPUT slot 94 (512 bytes) ===");
    let r = api.op("dump_fn_bytes", json!({"slot": 94, "count": 512}));
    println!("ok={} error={:?}", r.ok, r.error);
    if let Some(lines) = r.result["hex"].as_array() {
        for line in lines {
            println!("{}", line.as_str().unwrap_or(""));
        }
    }
    println!("params_size_double: {}", r.result["params_size_double"]);
    println!("params_size_float: {}", r.result["params_size_float"]);
}

#[test]
#[ignore = "scan exe for InputKey function address via string xref"]
fn find_inputkey_by_scan() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let r = api.op("find_inputkey", json!({}));
    println!("find_inputkey: ok={} error={:?}", r.ok, r.error);
    println!("{}", serde_json::to_string_pretty(&r.result).unwrap_or_default());
}

#[test]
#[ignore]
fn find_inputkey_ufunction() {
    let Some(api) = api_or_skip() else { return };

    for class in ["PlayerController", "PlayerInput", "EnhancedPlayerInput"] {
        let r = api.op("class_functions_by_name", json!({"class": class}));
        if !r.ok {
            println!("{class}: lookup failed: {:?}", r.error);
            continue;
        }
        let Some(functions) = r.result["functions"].as_array() else {
            println!("{class}: no functions array");
            continue;
        };
        let matches: Vec<_> = functions
            .iter()
            .filter(|f| {
                let name = f["name"].as_str().unwrap_or("");
                name.contains("Input") && name.contains("Key")
                    || name == "InputKey"
                    || name.contains("InputKey")
            })
            .collect();
        if matches.is_empty() {
            println!("{class}: no InputKey-related UFunctions found among {} total", functions.len());
        } else {
            println!("{class}: found {} InputKey-related UFunctions:", matches.len());
            for f in &matches {
                let name = f["name"].as_str().unwrap_or("?");
                let parms = f["parms_size"].as_u64().unwrap_or(0);
                let num = f["num_parms"].as_u64().unwrap_or(0);
                println!("  {name} ({num} parms, {parms} bytes)");
                print_function_parameters(&api, class, name);
            }
        }
    }
}

#[test]
#[ignore]
fn dump_playerinput_vtable() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player")
        .expect("no live player");
    let controller_addr = client::read_u64(&api, player.addr, 0x2C8);
    assert_ne!(controller_addr, 0, "no controller");
    let pi_addr = client::read_u64(&api, controller_addr, 0x408);
    assert_ne!(pi_addr, 0, "no PlayerInput");

    let pi_class = read_object_class_name(&api, pi_addr);
    println!("PlayerInput object: 0x{pi_addr:X} class={pi_class}");

    let vtable_ptr = client::read_u64(&api, pi_addr, 0x00);
    println!("vtable ptr: 0x{vtable_ptr:X}");

    let image_base = api.op("image_base", serde_json::json!({}));
    let base = image_base.result["base"].as_u64().unwrap_or(0);
    println!("image base: 0x{base:X}");

    println!("\nvtable entries (first 120 slots):");
    let mut unique_addrs: Vec<(u64, u64)> = Vec::new();
    for i in 0u64..120 {
        let slot_addr = vtable_ptr + i * 8;
        let fn_addr = client::read_u64(&api, slot_addr, 0x00);
        if fn_addr == 0 {
            println!("  [{i:3}] NULL (end of vtable?)");
            break;
        }
        println!("  [{i:3}] 0x{fn_addr:X}");
        if !unique_addrs.iter().any(|(_, a)| *a == fn_addr) {
            unique_addrs.push((i, fn_addr));
        }
    }

    let enhanced_input_slots: Vec<(u64, u64)> = unique_addrs
        .iter()
        .filter(|(_, addr)| *addr >= 0x7FF6A5590000 && *addr < 0x7FF6A5C00000)
        .copied()
        .collect();
    println!("\n--- EnhancedInput plugin functions ({} unique) ---", enhanced_input_slots.len());
    for (slot, addr) in &enhanced_input_slots {
        let bytes = client::read_bytes(&api, *addr, 0, 256);
        if bytes.is_empty() {
            println!("[{slot:3}] 0x{addr:X}: read failed");
            continue;
        }
        let hex = hex::encode(&bytes);
        println!("[{slot:3}] 0x{addr:X} ({} bytes):\n  {}", bytes.len(), hex);
    }
}

#[test]
#[ignore]
fn call_inputkey_vtable() {
    let Some(api) = api_or_skip() else { return };
    assert!(offsets_live(&api), "MISERY offsets are not live");

    let player = client::resolve_selector(&api, "live_player")
        .expect("no live player");
    let sel = format!("addr:0x{:X}", player.addr);
    let pos_before = actor_location(&api, &sel);
    println!("position before: {:.1}, {:.1}, {:.1}", pos_before[0], pos_before[1], pos_before[2]);

    let slot: u64 = 0; // unused, InputKey found by patternsleuth now
    println!("calling InputKey (found by patternsleuth) on controller with W pressed...");
    let r = api.op("try_inputkey", json!({
        "slot": slot,
        "key_name": "W",
        "pressed": true,
    }));
    println!("press result: ok={} result={} error={:?}", r.ok, r.result, r.error);
    if !r.ok {
        println!("FAILED: try_inputkey returned error");
        return;
    }

    std::thread::sleep(std::time::Duration::from_secs(2));

    let r = api.op("try_inputkey", json!({
        "slot": slot,
        "key_name": "W",
        "pressed": false,
    }));
    println!("release result: ok={} result={} error={:?}", r.ok, r.result, r.error);

    let pos_after = actor_location(&api, &sel);
    println!("position after:  {:.1}, {:.1}, {:.1}", pos_after[0], pos_after[1], pos_after[2]);

    let dx = pos_after[0] - pos_before[0];
    let dy = pos_after[1] - pos_before[1];
    let dz = pos_after[2] - pos_before[2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
    println!("distance moved: {dist:.1}");

    if dist > 1.0 {
        println!("SUCCESS: player moved {dist:.1} units via InputKey at vtable slot {slot}");
    } else {
        println!("FAILED: player did not move (dist={dist:.4}). Try a different slot.");
    }
}
