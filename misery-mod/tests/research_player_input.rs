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
