//! Can we ask the game which squares are loaded, instead of
//! reading every object to work it out?
//!
//! `spawning` used to search the whole object list on a timer to
//! answer that. Measured 2026-08-26: 174,000 to 230,000 objects
//! and 94 to 132 ms per search, on the game thread, six to eight
//! frames each (docs/performance.md).
//!
//! The game already keeps the list. `BP_WorldGeneration_Base_C`
//! has `StreamingLevels`, an array of `ULevelStreaming*`
//! (worldgen.md 2), and each entry points at its loaded `ULevel`,
//! whose name IS the square.
//!
//! This proved that, and `ueforge::ue::streaming` is what came of
//! it. Everything here is READ-ONLY.
//!
//! Run with a world loaded:
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_streaming -- --test-threads=1 --nocapture
//! ```

mod common;
use common::api_or_skip;
use modforge::client as c;
use serde_json::json;

const GENERATOR: &str = "BP_WorldGeneration_Base_C";

/// worldgen.md 2 and 10, all measured live.
const STREAMING_LEVELS: u64 = 0x2E8;
const TILE_SIZE: u64 = 0x2C0;
const EMISSIONS_PAST: u64 = 0x2F8;
const LOADED_LEVEL: u64 = 0x158;

/// What the generators hold right now.
///
/// Four exist, one per area (worldgen.md 1). Only the active one
/// streams anything, so the one with a non-empty array is the one
/// that matters.
#[test]
fn what_the_generators_are_streaming() {
    let Some(api) = api_or_skip() else { return };
    for g in generators(&api) {
        let (ptr, num, max) = c::read_tarray(&api, g.addr, STREAMING_LEVELS);
        println!("\n=== {}", g.name);
        println!("  StreamingLevels: {num} of {max} at {ptr:#x}");
        println!(
            "  TileSize: {}   EmissionsPast: {}",
            c::read_f64(&api, g.addr, TILE_SIZE),
            c::read_i32(&api, g.addr, EMISSIONS_PAST)
        );
    }
}

/// What is IN that array?
#[test]
fn each_streaming_level_names_its_square() {
    let Some(api) = api_or_skip() else { return };
    let Some(g) = active_generator(&api) else {
        println!("no generator is streaming anything; load a world");
        return;
    };
    println!("active generator: {}", g.name);

    let (ptr, num, _) = c::read_tarray(&api, g.addr, STREAMING_LEVELS);
    println!("{num} streaming level(s)\n");
    for i in 0..num.min(64) {
        let entry = c::read_tarray_entry(&api, ptr, i);
        if entry == 0 {
            println!("  [{i:>2}] null");
            continue;
        }
        // `inspect_address` answers nothing for these, so the
        // header is read instead (worldgen.md 10).
        let class = c::object_class(&api, entry)
            .and_then(|ca| c::object_name(&api, ca))
            .unwrap_or_else(|| "<none>".into());
        println!(
            "  [{i:>2}] {entry:#x}  name={:<44} class={class}",
            c::object_name(&api, entry).unwrap_or_else(|| "<none>".into())
        );
    }
}

/// Where is the loaded level?
///
/// **Do NOT use `discover_class_detail` for this.** It CRASHES
/// the game on `LevelStreamingDynamic`: an access violation
/// inside `UClass::iter_native_properties` reading `0x13afd5`,
/// far too small to be a real pointer (worldgen.md 10).
///
/// **And do NOT chase pointers found in memory.** The first
/// attempt read each 8-byte slot and asked what it pointed at.
/// Offset 0 of any UObject is its VTABLE, so that asked the name
/// table about garbage, which panicked and took the process down.
///
/// So: nothing is dereferenced. The addresses of the live levels
/// are collected first, then the streaming object's bytes are
/// read ONCE and compared against that list. A match is the
/// field. Comparison cannot fault.
#[test]
fn the_fields_that_lead_to_a_squares_actors() {
    let Some(api) = api_or_skip() else { return };
    let Some(g) = active_generator(&api) else {
        println!("no generator is streaming anything; load a world");
        return;
    };
    let (ptr, num, _) = c::read_tarray(&api, g.addr, STREAMING_LEVELS);
    if num == 0 {
        return;
    }

    // The class-chain search matches a SUBSTRING, so "Level" also
    // brings back LevelStreamingDynamic and friends. A real
    // level's full name starts with its class.
    let all = c::walk_class_chain_instances(&api, "Level", 512);
    let levels: Vec<(u64, String)> = all
        .iter()
        .filter(|w| w.full_name.starts_with("Level "))
        .map(|w| (w.addr as u64, w.full_name.clone()))
        .collect();
    println!("{} matched \"Level\", {} are levels", all.len(), levels.len());
    for (a, n) in levels.iter().take(6) {
        println!("   {a:#x}  {n}");
    }

    for i in 0..num.min(8) {
        let entry = c::read_tarray_entry(&api, ptr, i);
        if entry == 0 {
            continue;
        }
        println!("\n-- [{i}] {entry:#x}");
        let bytes = c::read_bytes(&api, entry, 0, 0x400);
        for off in (0..bytes.len().saturating_sub(8)).step_by(8) {
            let v = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
            if let Some((_, name)) = levels.iter().find(|(a, _)| *a == v) {
                println!("   +{off:#05x} -> {name}");
            }
        }
    }
}

/// Is the generator pointer worth caching?
#[test]
fn the_generator_pointer_holds_still() {
    let Some(api) = api_or_skip() else { return };
    let first: Vec<u64> = generators(&api).into_iter().map(|g| g.addr).collect();
    println!("first:  {first:x?}");
    println!("move around for 10 seconds");
    std::thread::sleep(std::time::Duration::from_secs(10));
    let second: Vec<u64> = generators(&api).into_iter().map(|g| g.addr).collect();
    println!("second: {second:x?}");
    assert_eq!(
        first, second,
        "the generator moved, so its pointer cannot be cached across streaming"
    );
    println!("\nstable: find it once, cache the pointer, never search again");
}

/// The whole chain, as the framework now walks it: no object
/// search anywhere.
#[test]
fn the_chain_with_no_search_in_it() {
    let Some(api) = api_or_skip() else { return };
    let Some(g) = active_generator(&api) else {
        println!("no generator is streaming anything; load a world");
        return;
    };
    let (ptr, num, _) = c::read_tarray(&api, g.addr, STREAMING_LEVELS);
    let mut squares = Vec::new();
    for i in 0..num {
        let entry = c::read_tarray_entry(&api, ptr, i);
        if entry == 0 {
            continue;
        }
        let level = c::read_u64(&api, entry, LOADED_LEVEL);
        if level == 0 {
            continue;
        }
        // The level's OWN name is `PersistentLevel` for every
        // one of them. The square is in its outers.
        squares.push(c::object_full_name(&api, level));
    }
    println!("{} loaded square(s):", squares.len());
    for s in &squares {
        println!("   {s}");
    }
    assert!(
        !squares.is_empty(),
        "a world is loaded, so the chain should name at least one square"
    );
}

/// How much cheaper is asking the generator than searching?
#[test]
fn the_size_of_the_thing_we_are_avoiding() {
    let Some(api) = api_or_skip() else { return };
    let on = api.op("timing", json!({ "on": true, "reset": true }));
    assert!(on.ok, "could not switch timing on: {:?}", on.error);

    let search = api.op(
        "walk_class_chain",
        json!({"needle": "BP_MasterAICharacter_C", "max": 4}),
    );
    assert!(search.ok, "search failed: {:?}", search.error);

    let report = api.op("timing_report", json!({}));
    let off = api.op("timing", json!({ "on": false, "reset": false }));
    assert!(off.ok, "could not switch timing off: {:?}", off.error);

    let mut read = 0u64;
    let mut ms = 0.0;
    for e in report.result["entries"].as_array().cloned().unwrap_or_default() {
        match e["name"].as_str().unwrap_or("") {
            "ue:objects_read" => read = e["calls"].as_u64().unwrap_or(0),
            "ue:find_objects_by_chain" => ms = e["total_ms"].as_f64().unwrap_or(0.0),
            _ => {}
        }
    }
    let squares = active_generator(&api)
        .map(|g| c::read_tarray(&api, g.addr, STREAMING_LEVELS).1)
        .unwrap_or(0);

    println!("\none search:  {read} objects read, {ms:.1} ms");
    println!("the array:   {squares} entries, one pointer read");
}

/// A live generator: its address and what it is called.
struct Live {
    addr: u64,
    name: String,
}

fn generators(api: &common::Api) -> Vec<Live> {
    c::walk_class_chain_instances(api, GENERATOR, 16)
        .into_iter()
        .filter(|w| w.full_name.contains("PersistentLevel"))
        .map(|w| Live { addr: w.addr as u64, name: w.full_name })
        .collect()
}

/// The generator actually streaming squares.
fn active_generator(api: &common::Api) -> Option<Live> {
    generators(api)
        .into_iter()
        .find(|g| c::read_tarray(api, g.addr, STREAMING_LEVELS).1 > 0)
}
