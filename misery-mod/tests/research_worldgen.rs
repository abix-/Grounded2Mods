//! World generation research (worldgen.md).
//!
//! Forced regeneration: BP_GlobalManager_C:GenerateCustomBiom
//! takes one byte (the area number) and rebuilds the expedition
//! world on demand through the game-thread call op. Running it
//! for 0..=3 maps every number to its area (worldgen.md 5, 7).
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_worldgen -- --test-threads=1 --nocapture
//! MISERY_BIOME=2 MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_worldgen -- --ignored --test-threads=1 --nocapture force_regenerate
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use modforge::client;
use std::time::Duration;

type Api = common::Api;

// BP_GlobalManager_C fields (worldgen.md 5).
const CURRENT_GENERATED_LEVEL: u64 = 0x2C8;
const CURRENT_WORLD_SEED: u64 = 0x2BC;
// BP_WorldGeneration_Base_C fields (worldgen.md 2).
const STREAMING_LEVELS: u64 = 0x2E8;
const EMISSIONS_PAST: u64 = 0x2F8;

fn manager(api: &Api) -> Option<client::ClassInstance> {
    client::walk_class_chain_instances(api, "BP_GlobalManager_C", 2)
        .into_iter()
        .next()
}

fn read_u8_at(api: &Api, addr: u64, off: u64) -> u8 {
    client::read_bytes(api, addr, off, 1).first().copied().unwrap_or(255)
}

fn read_u32_at(api: &Api, addr: u64, off: u64) -> u32 {
    let b = client::read_bytes(api, addr, off, 4);
    if b.len() == 4 { client::from_le_u32(&b, 0) } else { 0 }
}

/// Per-generator snapshot: (name, streaming level count, emissions).
fn generator_state(api: &Api) -> Vec<(String, i32, i32)> {
    client::walk_class_chain_instances(api, "BP_WorldGeneration_Base_C", 8)
        .iter()
        .map(|g| {
            let streaming = client::read_tarray_header(api, g.addr, STREAMING_LEVELS)
                .map(|h| h.num)
                .unwrap_or(-1);
            let b = client::read_bytes(api, g.addr, EMISSIONS_PAST, 4);
            let em = if b.len() == 4 { client::from_le_i32(&b, 0) } else { -1 };
            (g.name.clone(), streaming, em)
        })
        .collect()
}

fn print_state(api: &Api, label: &str) {
    if let Some(m) = manager(api) {
        println!(
            "{label}: manager current_level={} seed={}",
            read_u8_at(api, m.addr, CURRENT_GENERATED_LEVEL),
            read_u32_at(api, m.addr, CURRENT_WORLD_SEED),
        );
    } else {
        println!("{label}: no BP_GlobalManager_C live");
    }
    for (name, streaming, em) in generator_state(api) {
        println!("{label}:   {name} streaming={streaming} emissions={em}");
    }
}

/// Read-only snapshot of the worldgen state.
#[test]
fn dump_worldgen_state() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    print_state(&api, "state");
}

/// Force a world regeneration via GenerateCustomBiom on the
/// game thread. MISERY_BIOME selects the area number (default:
/// the manager's current value, so a plain run just regenerates
/// the current area). Rebuilds the expedition world: run while
/// safe in the hub.
#[test]
#[ignore = "rebuilds the expedition world"]
fn force_regenerate() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let Some(m) = manager(&api) else {
        panic!("no BP_GlobalManager_C live (known to disappear after regen; reload the save)");
    };
    let current = read_u8_at(&api, m.addr, CURRENT_GENERATED_LEVEL);
    let target: u8 = std::env::var("MISERY_BIOME")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(current);

    print_state(&api, "before");
    println!("calling GenerateCustomBiom({target}) (was {current})");

    let (out, _) = api
        .call_ufunction(
            "BP_GlobalManager_C",
            "GenerateCustomBiom",
            &m.addr_selector,
            &[target],
        )
        .expect("GenerateCustomBiom call failed");
    println!("call returned, parms after: {out:02x?}");

    // Generation streams levels over several seconds.
    for i in 0..6 {
        std::thread::sleep(Duration::from_secs(5));
        println!("--- {}s ---", (i + 1) * 5);
        print_state(&api, "after");
    }
}
