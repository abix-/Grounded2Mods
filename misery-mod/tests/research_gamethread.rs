//! Reaching the game thread at the main menu.
//!
//! `ueforge::frame::on_update` (UE4SS's `on_update`) fires every
//! frame but runs on UE4SS's own thread, not the game thread
//! (research.md 26.6), so it cannot call UFunctions. The settled
//! answer in Unreal modding is to hook `UEngine::Tick`, which
//! runs once per frame on the game thread whether or not a world
//! is loaded. UE4SS does exactly that:
//!
//! ```ini
//! HookEngineTick = 1
//! EngineTickResolveMethod = Scan   ; patternsleuth, vtable fallback
//! ```
//!
//! Its C++ mod API exposes no way to subscribe to that existing
//! hook (`ueforge_cppusermodbase.hpp` has only the lifecycle
//! virtuals), so the mod must resolve `UEngine::Tick` itself.
//!
//! These tests are read-only groundwork for that: find the live
//! engine object and read its vtable.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_gamethread -- --test-threads=1 --nocapture
//! ```

mod common;
use common::api_or_skip;
use serde_json::json;

/// Read `count` pointer-sized entries starting at `addr`.
fn read_ptrs(api: &common::Api, addr: u64, count: usize) -> Vec<u64> {
    let r = api.op(
        "read_bytes",
        json!({
            "instance_selector": format!("addr:0x{addr:X}"),
            "offset": 0,
            "length": count * 8,
        }),
    );
    if !r.ok {
        println!("read_bytes at 0x{addr:X} failed: {:?}", r.error);
        return Vec::new();
    }
    let hex = r.result["bytes_hex"].as_str().unwrap_or("");
    let bytes: Vec<u8> = (0..hex.len() / 2)
        .filter_map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok())
        .collect();
    bytes
        .chunks_exact(8)
        .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

/// Which engine objects are live, and what is at the front of
/// each one? The first 8 bytes of any UObject are its vtable
/// pointer, and `UEngine::Tick` is one of the slots in it.
#[test]
fn find_engine() {
    let Some(api) = api_or_skip() else { return };
    for needle in ["GameEngine", "Engine"] {
        let live = modforge::client::walk_class_chain_instances(&api, needle, 16);
        println!("\n=== class chain contains {needle:?}: {} live", live.len());
        for o in &live {
            println!("  {} @ 0x{:X}", o.full_name, o.addr);
        }
    }
}

/// The engine's vtable, so the Tick slot can be identified.
///
/// Prints the first slots as raw addresses. A later step decides
/// which one is Tick, either by patternsleuth scanning for the
/// function or by matching a known index.
#[test]
fn engine_vtable() {
    let Some(api) = api_or_skip() else { return };
    let live = modforge::client::walk_class_chain_instances(&api, "GameEngine", 8);
    let Some(engine) = live
        .iter()
        .find(|o| o.full_name.contains("/Engine/Transient"))
    else {
        println!("no live engine object under /Engine/Transient");
        for o in &live {
            println!("  saw: {}", o.full_name);
        }
        return;
    };
    println!("engine: {} @ 0x{:X}", engine.full_name, engine.addr);

    let head = read_ptrs(&api, engine.addr, 1);
    let Some(&vtable) = head.first() else {
        println!("could not read the engine's vtable pointer");
        return;
    };
    println!("vtable: 0x{vtable:X}");

    // UE4SS resolves GameEngine::Tick for itself and logs the
    // address, by patternsleuth scan AND by vtable lookup, which
    // agreed. Set MISERY_ENGINE_TICK to that address to learn
    // its vtable INDEX, which is the part that carries to other
    // UE 5.4 games where the scan may fail.
    let target = std::env::var("MISERY_ENGINE_TICK")
        .ok()
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok());

    let slots = read_ptrs(&api, vtable, 400);
    println!("{} slots read", slots.len());
    for (i, s) in slots.iter().enumerate() {
        if Some(*s) == target {
            println!("  [{i:>3}] 0x{s:X}   <== GameEngine::Tick");
        }
    }
    if let Some(t) = target {
        if !slots.contains(&t) {
            println!("0x{t:X} not found in the first {} slots", slots.len());
        }
    } else {
        for (i, s) in slots.iter().enumerate().take(64) {
            println!("  [{i:>3}] 0x{s:X}");
        }
    }
}
