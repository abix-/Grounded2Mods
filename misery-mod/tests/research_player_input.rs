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
