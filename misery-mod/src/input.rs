//! MISERY connection to Ueforge player input.

use std::ffi::c_void;
use std::time::Duration;

pub fn register() {
    ueforge::input::register("misery", &crate::speed::PLAYER);
    register_ops();
}

fn register_ops() {
    ueforge::ops::OP_REGISTRY.register_many([
        ueforge::ops::OpDef::new(
            "try_inputkey",
            "Call InputKey at a vtable slot on the PlayerInput object (research)",
            "{slot: u64, key_name: str, pressed: bool}",
            |args| {
                let slot = args["slot"].as_u64().ok_or("missing slot")? as usize;
                let key_name =
                    args["key_name"].as_str().ok_or("missing key_name")?.to_string();
                let pressed = args["pressed"].as_bool().ok_or("missing pressed")?;
                let dry_run = args["dry_run"].as_bool().unwrap_or(false);
                ueforge::game_thread::run(
                    move || {
                        if dry_run {
                            dry_run_inputkey(slot, &key_name)
                        } else {
                            try_inputkey(slot, &key_name, pressed)
                        }
                    },
                    Duration::from_secs(5),
                )
            },
        ),
        ueforge::ops::OpDef::new(
            "find_inputkey",
            "Scan the exe for InputKey function address via string xref (research)",
            "{}",
            |_args| find_inputkey_by_scan(),
        ),
        ueforge::ops::OpDef::new(
            "dump_fn_bytes",
            "Read raw bytes from a vtable slot's function (research)",
            "{target: str, slot: u64, count: u64}",
            |args| {
                let target = args["target"].as_str().unwrap_or("playerinput").to_string();
                let slot = args["slot"].as_u64().ok_or("missing slot")? as usize;
                let count = args["count"].as_u64().unwrap_or(256) as usize;
                ueforge::game_thread::run(
                    move || dump_fn_bytes(&target, slot, count),
                    Duration::from_secs(5),
                )
            },
        ),
    ]);
}

fn dump_fn_bytes(target: &str, slot: usize, count: usize) -> Result<serde_json::Value, String> {
    let player = crate::speed::PLAYER
        .retained()
        .ok_or("no retained player")?;

    let controller_addr: usize = unsafe { player.read_field(0x2C8) };
    if controller_addr == 0 {
        return Err("player has no controller at +0x2C8".into());
    }
    let controller = unsafe { &*(controller_addr as *const ueforge::ue::UObject) };

    let obj: &ueforge::ue::UObject = match target {
        "controller" => controller,
        _ => {
            let pi_addr: usize = unsafe { controller.read_field(0x408) };
            if pi_addr == 0 {
                return Err("controller has no PlayerInput at +0x408".into());
            }
            unsafe { &*(pi_addr as *const ueforge::ue::UObject) }
        }
    };

    let fn_ptr = unsafe { obj.vtable_fn(slot) };
    let count = count.min(4096);
    let bytes: Vec<u8> =
        unsafe { std::slice::from_raw_parts(fn_ptr as *const u8, count).to_vec() };

    let hex_lines: Vec<String> = bytes
        .chunks(16)
        .enumerate()
        .map(|(i, chunk)| {
            let hex = chunk.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
            format!("+0x{:03x}: {}", i * 16, hex)
        })
        .collect();

    Ok(serde_json::json!({
        "target": target,
        "slot": slot,
        "fn_addr": format!("0x{:x}", fn_ptr as usize),
        "byte_count": bytes.len(),
        "hex": hex_lines,
        "params_size_double": std::mem::size_of::<FInputKeyParams>(),
        "params_size_float": std::mem::size_of::<FInputKeyParamsFloat>(),
    }))
}

fn find_inputkey_by_scan() -> Result<serde_json::Value, String> {
    use modforge::patterns::sleuth::{scan_all_matches, scan_rdata_matches};

    // Search for multiple strings that InputKey might reference.
    // UE stores strings as UTF-16. Try several candidates.
    let search_strings = [
        "InputKey\0",
        "APlayerController::InputKey\0",
        "IE_Pressed\0",
        "FInputKeyParams\0",
    ];

    let mut all_results: Vec<serde_json::Value> = Vec::new();

    for search_str in &search_strings {
        let utf16_bytes: Vec<u8> = search_str
            .encode_utf16()
            .flat_map(|c| c.to_le_bytes())
            .collect();
        let rdata_sig: String = utf16_bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ");

        let rdata_hits = scan_rdata_matches(&rdata_sig)
            .map_err(|e| format!("rdata scan for '{search_str}' failed: {e}"))?;

        // Also try UTF-8 version
        let utf8_sig: String = search_str
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        let utf8_hits = scan_rdata_matches(&utf8_sig)
            .map_err(|e| format!("rdata utf8 scan for '{search_str}' failed: {e}"))?;

        let combined: Vec<usize> = rdata_hits.iter().chain(utf8_hits.iter()).copied().collect();

        for &str_addr in &combined {
            // Find xrefs: lea with RIP-relative addressing
            let xref_patterns = [
                format!("48 8d ?? X0x{str_addr:X}"),
                format!("4c 8d ?? X0x{str_addr:X}"),
            ];
            for pat in &xref_patterns {
                let hits = match scan_all_matches(pat) {
                    Ok(h) => h,
                    Err(_) => continue,
                };
                for &xref_addr in &hits {
                    // Walk backward to find function entry (CC padding)
                    let mut fn_addr: usize = 0;
                    let search_back: usize = 0x400;
                    let start = xref_addr.saturating_sub(search_back);
                    let window = unsafe {
                        std::slice::from_raw_parts(start as *const u8, xref_addr - start)
                    };
                    for i in (1..window.len()).rev() {
                        if window[i - 1] == 0xCC && window[i] != 0xCC {
                            fn_addr = start + i;
                            break;
                        }
                    }

                    let first_bytes: Vec<u8> = if fn_addr != 0 {
                        unsafe {
                            std::slice::from_raw_parts(fn_addr as *const u8, 16).to_vec()
                        }
                    } else {
                        Vec::new()
                    };

                    all_results.push(serde_json::json!({
                        "search_string": search_str,
                        "string_addr": format!("0x{str_addr:x}"),
                        "xref_addr": format!("0x{xref_addr:x}"),
                        "fn_entry": format!("0x{fn_addr:x}"),
                        "fn_first_bytes": first_bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                    }));
                }
            }
        }

        if combined.is_empty() {
            all_results.push(serde_json::json!({
                "search_string": search_str,
                "string_found": false,
            }));
        } else if all_results.iter().all(|r| {
            r["search_string"].as_str() != Some(search_str)
                || r.get("string_found").is_some()
        }) {
            all_results.push(serde_json::json!({
                "search_string": search_str,
                "string_found": true,
                "string_addrs": combined.iter().map(|a| format!("0x{a:x}")).collect::<Vec<_>>(),
                "xrefs_found": 0,
            }));
        }
    }

    // Check vtable matches for any found function entries
    let mut vtable_matches: Vec<serde_json::Value> = Vec::new();
    if let Some(player) = crate::speed::PLAYER.retained() {
        let controller_addr: usize = unsafe { player.read_field(0x2C8) };
        if controller_addr != 0 {
            let controller =
                unsafe { &*(controller_addr as *const ueforge::ue::UObject) };
            let pi_addr: usize = unsafe { controller.read_field(0x408) };

            let fn_entries: Vec<usize> = all_results
                .iter()
                .filter_map(|r| {
                    r["fn_entry"].as_str().and_then(|s| {
                        usize::from_str_radix(s.trim_start_matches("0x"), 16).ok()
                    })
                })
                .filter(|&a| a != 0)
                .collect();

            // Check controller vtable (up to 200 slots)
            for slot in 0..200usize {
                let slot_fn = unsafe { controller.vtable_fn(slot) } as usize;
                if fn_entries.contains(&slot_fn) {
                    vtable_matches.push(serde_json::json!({
                        "object": "controller",
                        "slot": slot,
                        "fn_addr": format!("0x{slot_fn:x}"),
                    }));
                }
            }

            // Check PlayerInput vtable (up to 120 slots)
            if pi_addr != 0 {
                let player_input =
                    unsafe { &*(pi_addr as *const ueforge::ue::UObject) };
                for slot in 0..120usize {
                    let slot_fn = unsafe { player_input.vtable_fn(slot) } as usize;
                    if fn_entries.contains(&slot_fn) {
                        vtable_matches.push(serde_json::json!({
                            "object": "playerinput",
                            "slot": slot,
                            "fn_addr": format!("0x{slot_fn:x}"),
                        }));
                    }
                }
            }
        }
    }

    // Also try: find the function by scanning for its known prologue bytes.
    // Controller slot 130 has prologue: 40 55 57 41 57 48 8d 6c 24 b9
    // followed by f6 41 58 10 (test byte [rcx+0x58], 0x10)
    // and 48 8b 81 a0 01 00 00 (mov rax, [rcx+0x1A0] = PlayerInput)
    // This combination is likely unique to APlayerController::InputKey.
    let prologue_sig = "40 55 57 41 57 48 8d 6c 24 b9 48 81 ec f0 00 00 00 f6 41 58 10";
    let prologue_hits = scan_all_matches(prologue_sig)
        .map_err(|e| format!("prologue scan failed: {e}"))?;

    // For each hit, check if it reads PlayerInput at [rcx+0x1A0]
    let mut prologue_results: Vec<serde_json::Value> = Vec::new();
    for &addr in &prologue_hits {
        let fn_bytes: Vec<u8> = unsafe {
            std::slice::from_raw_parts(addr as *const u8, 64).to_vec()
        };
        let hex = fn_bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        prologue_results.push(serde_json::json!({
            "fn_addr": format!("0x{addr:x}"),
            "first_64_bytes": hex,
        }));
    }

    Ok(serde_json::json!({
        "results": all_results,
        "vtable_matches": vtable_matches,
        "prologue_scan": {
            "sig": prologue_sig,
            "hits": prologue_results,
        },
    }))
}

fn dry_run_inputkey(
    slot: usize,
    key_name: &str,
) -> Result<serde_json::Value, String> {
    let player = crate::speed::PLAYER
        .retained()
        .ok_or("no retained player")?;

    let controller_addr: usize = unsafe { player.read_field(0x2C8) };
    if controller_addr == 0 {
        return Err("player has no controller at +0x2C8".into());
    }
    let controller = unsafe { &*(controller_addr as *const ueforge::ue::UObject) };

    let pi_addr: usize = unsafe { controller.read_field(0x408) };
    if pi_addr == 0 {
        return Err("controller has no PlayerInput at +0x408".into());
    }
    let player_input = unsafe { &*(pi_addr as *const ueforge::ue::UObject) };

    let fname = ueforge::ue::fname::from_str(key_name, ueforge::ue::fname::FindName::Find)
        .ok_or_else(|| format!("FName not found for key '{key_name}'"))?;

    let fn_ptr = unsafe { player_input.vtable_fn(slot) };
    let first_bytes: [u8; 16] =
        unsafe { std::ptr::read_unaligned(fn_ptr as *const [u8; 16]) };

    Ok(serde_json::json!({
        "slot": slot,
        "fn_addr": format!("{:#x}", fn_ptr as usize),
        "first_bytes": first_bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "fname_ci": fname.comparison_index,
        "fname_num": fname.number,
        "params_size": std::mem::size_of::<FInputKeyParams>(),
    }))
}

#[repr(C)]
struct FInputKeyParams {
    // FKey: FName (8 bytes) + TSharedPtr<FKeyDetails> (16 bytes)
    key_fname_ci: i32,
    key_fname_num: u32,
    key_details_ptr: usize,
    key_details_ref: usize,
    // FVector2D Delta (UE5.4 LWC: double)
    delta_x: f64,
    delta_y: f64,
    // float DeltaTime
    delta_time: f32,
    // int32 NumSamples
    num_samples: i32,
    // EInputEvent
    event: i32,
    // FInputDeviceId
    input_device: i32,
    // bool bIsGamepadOverride
    is_gamepad: u8,
    _pad: [u8; 7],
}

#[repr(C)]
struct FInputKeyParamsFloat {
    // FKey: FName (8 bytes) + TSharedPtr<FKeyDetails> (16 bytes)
    key_fname_ci: i32,
    key_fname_num: u32,
    key_details_ptr: usize,
    key_details_ref: usize,
    // FVector2D Delta (float version, 8 bytes total)
    delta_x: f32,
    delta_y: f32,
    // float DeltaTime
    delta_time: f32,
    // int32 NumSamples
    num_samples: i32,
    // EInputEvent
    event: i32,
    // FInputDeviceId
    input_device: i32,
    // bool bIsGamepadOverride
    is_gamepad: u8,
    _pad: [u8; 3],
}

fn try_inputkey(
    slot: usize,
    key_name: &str,
    pressed: bool,
) -> Result<serde_json::Value, String> {
    use modforge::patterns::sleuth::scan_all_matches;

    let player = crate::speed::PLAYER
        .retained()
        .ok_or("no retained player")?;

    let controller_addr: usize = unsafe { player.read_field(0x2C8) };
    if controller_addr == 0 {
        return Err("player has no controller at +0x2C8".into());
    }

    // Find InputKey by patternsleuth signature scan (unique prologue)
    let sig = "40 55 57 41 57 48 8d 6c 24 b9 48 81 ec f0 00 00 00 f6 41 58 10";
    let hits = scan_all_matches(sig)
        .map_err(|e| format!("patternsleuth scan failed: {e}"))?;
    if hits.is_empty() {
        return Err("InputKey function not found by patternsleuth scan".into());
    }
    if hits.len() > 1 {
        return Err(format!("InputKey signature matched {} addresses (expected 1)", hits.len()));
    }
    let fn_ptr = hits[0] as *const c_void;

    let fname = ueforge::ue::fname::from_str(key_name, ueforge::ue::fname::FindName::Find)
        .ok_or_else(|| format!("FName not found for key '{key_name}'"))?;

    let event: i32 = if pressed { 0 } else { 1 };
    let params = FInputKeyParams {
        key_fname_ci: fname.comparison_index,
        key_fname_num: fname.number,
        key_details_ptr: 0,
        key_details_ref: 0,
        delta_x: 0.0,
        delta_y: 0.0,
        delta_time: 0.0,
        num_samples: 1,
        event,
        input_device: 0,
        is_gamepad: 0,
        _pad: [0; 7],
    };

    type InputKeyFn = unsafe extern "system" fn(*const c_void, *const c_void) -> bool;

    let input_key: InputKeyFn = unsafe { std::mem::transmute(fn_ptr) };
    let result = unsafe {
        input_key(
            controller_addr as *const c_void,
            &params as *const FInputKeyParams as *const c_void,
        )
    };

    Ok(serde_json::json!({
        "key": key_name,
        "pressed": pressed,
        "fn_addr": format!("{:#x}", fn_ptr as usize),
        "returned": result,
    }))
}
