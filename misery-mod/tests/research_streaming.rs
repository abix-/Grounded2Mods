//! Can we ask the game which squares are loaded, instead of
//! reading every object to work it out?
//!
//! `spawning` and `strange` both search the whole object list on
//! a timer to answer "which map squares are loaded". Measured
//! 2026-08-26: 174,000 to 230,000 objects and 94 to 132 ms per
//! search, on the game thread, six to eight frames each
//! (docs/performance.md).
//!
//! The game already keeps that list. `BP_WorldGeneration_Base_C`
//! has `StreamingLevels`, an array of `ULevelStreaming*`
//! (worldgen.md 2). `strange.rs` already reads its LENGTH and
//! throws the contents away.
//!
//! Two things have to be true before the watchers can be rebuilt
//! around it, and this test answers both:
//!
//!   1. Does a `ULevelStreaming` name its square, and does it
//!      hand us the loaded `ULevel` with that square's own actor
//!      list? If so, "the NPCs in this square" is a short list
//!      rather than a search of the world.
//!   2. Does the generator pointer survive squares streaming in
//!      and out? If so it is found once and cached, and the last
//!      full search disappears too.
//!
//! Everything here is READ-ONLY.
//!
//! Run with a world loaded:
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_streaming -- --test-threads=1 --nocapture
//! ```

mod common;
use common::api_or_skip;
use serde_json::json;

const GENERATOR: &str = "BP_WorldGeneration_Base_C";

/// worldgen.md 2. A UE `TArray` is `{ void* Data; int32 Num;
/// int32 Max; }`, so 16 bytes.
const STREAMING_LEVELS: usize = 0x2E8;
const TILE_SIZE: usize = 0x2C0;
const EMISSIONS_PAST: usize = 0x2F8;
const TARRAY_BYTES: usize = 16;

/// `UObject` header, from `ueforge::ue::offsets`.
const OBJ_CLASS: usize = 0x10;
const OBJ_NAME: usize = 0x18;

/// How far into a `LevelStreamingDynamic` to look for its
/// fields. Generous: the object is bigger than this, but
/// everything interesting on a UE streaming level sits near the
/// front.
const SCAN_BYTES: usize = 0x400;

/// What the generators hold right now.
///
/// Four generators exist, one per area (worldgen.md 1). Only the
/// active one streams anything, so the one with a non-empty
/// `StreamingLevels` is the one that matters.
#[test]
fn what_the_generators_are_streaming() {
    let Some(api) = api_or_skip() else { return };
    for g in generators(&api) {
        println!("\n=== {}", g.full_name);
        let (ptr, num, max) = tarray(&api, &g.sel, STREAMING_LEVELS);
        let tile = read_f64(&api, &g.sel, TILE_SIZE);
        let emissions = read_i32(&api, &g.sel, EMISSIONS_PAST);
        println!("  StreamingLevels: {num} of {max} at {ptr:#x}");
        println!("  TileSize: {tile}   EmissionsPast: {emissions}");
    }
}

/// The whole point: what is IN that array, and does each entry
/// name its square?
///
/// If the names are the square names, the watchers never need to
/// search for a square again.
#[test]
fn each_streaming_level_names_its_square() {
    let Some(api) = api_or_skip() else { return };
    let Some(g) = active_generator(&api) else {
        println!("no generator is streaming anything; load a world");
        return;
    };
    println!("active generator: {}", g.full_name);

    let (ptr, num, _) = tarray(&api, &g.sel, STREAMING_LEVELS);
    println!("{num} streaming level(s)\n");
    for i in 0..num.min(64) {
        let entry = read_u64(&api, &format!("addr:0x{ptr:X}"), i * 8);
        if entry == 0 {
            println!("  [{i:>2}] null");
            continue;
        }
        // Read the object header rather than asking
        // `inspect_address`, which answers nothing useful for
        // these (same defect as the menu widgets, todo).
        let sel = format!("addr:0x{entry:X}");
        let class_ptr = read_u64(&api, &sel, OBJ_CLASS);
        println!(
            "  [{i:>2}] {entry:#x}  name={:<44} class={}",
            object_name(&api, &sel),
            if class_ptr == 0 {
                "<none>".to_string()
            } else {
                object_name(&api, &format!("addr:0x{class_ptr:X}"))
            }
        );
    }
}

/// A `UObject`'s own name, read out of its header and resolved
/// through the name table.
///
/// `NamePrivate` is an `FName`: a 4-byte index and a 4-byte
/// number (`ueforge::ue::offsets`).
fn object_name(api: &common::Api, sel: &str) -> String {
    let raw = read_u64(api, sel, OBJ_NAME);
    if raw == 0 {
        return "<none>".to_string();
    }
    let r = api.op("fname_to_string", json!({ "fname": raw }));
    if !r.ok {
        return format!("<fname {raw:#x} failed>");
    }
    r.result["string"]
        .as_str()
        .or_else(|| r.result.as_str())
        .unwrap_or("<?>")
        .to_string()
}

/// Where is the loaded level, and where is its actor list?
///
/// **Do NOT use `discover_class_detail` for this.** It CRASHES
/// the game on `LevelStreamingDynamic`, confirmed twice on
/// 2026-08-26 with a symbolised dump: an access violation inside
/// `UClass::iter_native_properties` reading `0x13afd5`, which is
/// far too small to be a real pointer. Its own description claims
/// it is safe from the eager-walk crash; it is not, for a native
/// engine class (worldgen.md 10).
///
/// **And do NOT chase pointers found in memory.** The first
/// attempt at this test read each 8-byte slot and asked what it
/// pointed at. Offset 0 of any UObject is its VTABLE, so that
/// asked `fname_to_string` about garbage, which panicked, and the
/// panic took the process down. Third crash of the evening, all
/// three caused by this test.
///
/// So: nothing is dereferenced. The addresses of every live level
/// are collected first, then the streaming object's bytes are
/// read ONCE and compared against that list. A match is the field
/// pointing at that square's loaded level. Comparison only, so
/// there is nothing here that can fault.
#[test]
fn the_fields_that_lead_to_a_squares_actors() {
    let Some(api) = api_or_skip() else { return };
    let Some(g) = active_generator(&api) else {
        println!("no generator is streaming anything; load a world");
        return;
    };
    let (ptr, num, _) = tarray(&api, &g.sel, STREAMING_LEVELS);
    if num == 0 {
        return;
    }
    // The addresses of every live level, and what each is called.
    // The name carries the square.
    // The search matches a class-chain SUBSTRING, so "Level"
    // also brings back LevelStreamingDynamic, LevelBounds and
    // friends. A real level's full name starts with the class,
    // so keep only those.
    let all = modforge::client::walk_class_chain_instances(&api, "Level", 512);
    let levels: Vec<(u64, String)> = all
        .iter()
        .filter(|w| w.full_name.starts_with("Level "))
        .map(|w| (w.addr as u64, w.full_name.clone()))
        .collect();
    println!("{} object(s) matched \"Level\", {} are levels", all.len(), levels.len());
    for (a, n) in levels.iter().take(6) {
        println!("   {a:#x}  {n}");
    }

    for i in 0..num.min(8) {
        let entry = read_u64(&api, &format!("addr:0x{ptr:X}"), i * 8);
        if entry == 0 {
            continue;
        }
        println!("\n-- [{i}] {entry:#x}");
        let bytes = read_bytes(&api, &format!("addr:0x{entry:X}"), 0, SCAN_BYTES);
        for off in (0..bytes.len().saturating_sub(8)).step_by(8) {
            let v = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
            if let Some((_, name)) = levels.iter().find(|(a, _)| *a == v) {
                println!("   +{off:#05x} -> {name}");
            }
        }
    }
}

/// Is the generator pointer worth caching?
///
/// Read it twice, several seconds apart, with the player moving.
/// Same address both times means it is found once and never
/// searched for again.
#[test]
fn the_generator_pointer_holds_still() {
    let Some(api) = api_or_skip() else { return };
    let first: Vec<String> = generators(&api).into_iter().map(|g| g.sel).collect();
    println!("first:  {first:?}");
    println!("move around for 10 seconds");
    std::thread::sleep(std::time::Duration::from_secs(10));
    let second: Vec<String> = generators(&api).into_iter().map(|g| g.sel).collect();
    println!("second: {second:?}");
    assert_eq!(
        first, second,
        "the generator moved, so its pointer cannot be cached across streaming"
    );
    println!("\nstable: find it once, cache the pointer, never search again");
}

/// How much cheaper is asking the generator than searching?
///
/// Counts the objects a search reads, against the length of the
/// array that holds the same answer.
#[test]
fn the_size_of_the_thing_we_are_avoiding() {
    let Some(api) = api_or_skip() else { return };
    let on = api.op("timing", json!({ "on": true, "reset": true }));
    assert!(on.ok, "could not switch timing on: {:?}", on.error);

    let search = api.op("walk_class_chain", json!({"needle": "BP_MasterAICharacter_C", "max": 4}));
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
        .map(|g| tarray(&api, &g.sel, STREAMING_LEVELS).1)
        .unwrap_or(0);

    println!("\none search:  {read} objects read, {ms:.1} ms");
    println!("the array:   {squares} entries, one pointer read");
}

/// A live object: its name and the selector to read it with.
struct Live {
    full_name: String,
    sel: String,
}

fn generators(api: &common::Api) -> Vec<Live> {
    modforge::client::walk_class_chain_instances(api, GENERATOR, 16)
        .into_iter()
        .filter(|w| w.full_name.contains("PersistentLevel"))
        .map(|w| Live { full_name: w.full_name, sel: w.addr_selector })
        .collect()
}

/// The generator that is actually streaming squares. Four exist,
/// one per area; only the active one has any.
fn active_generator(api: &common::Api) -> Option<Live> {
    generators(api)
        .into_iter()
        .find(|g| tarray(api, &g.sel, STREAMING_LEVELS).1 > 0)
}

/// Read a UE `TArray` header: `(data pointer, length, capacity)`.
fn tarray(api: &common::Api, sel: &str, offset: usize) -> (u64, usize, usize) {
    let b = read_bytes(api, sel, offset, TARRAY_BYTES);
    if b.len() < TARRAY_BYTES {
        return (0, 0, 0);
    }
    let ptr = u64::from_le_bytes(b[0..8].try_into().unwrap());
    let num = i32::from_le_bytes(b[8..12].try_into().unwrap());
    let max = i32::from_le_bytes(b[12..16].try_into().unwrap());
    (ptr, num.max(0) as usize, max.max(0) as usize)
}

fn read_u64(api: &common::Api, sel: &str, offset: usize) -> u64 {
    let b = read_bytes(api, sel, offset, 8);
    if b.len() < 8 {
        return 0;
    }
    u64::from_le_bytes(b[0..8].try_into().unwrap())
}

fn read_i32(api: &common::Api, sel: &str, offset: usize) -> i32 {
    let b = read_bytes(api, sel, offset, 4);
    if b.len() < 4 {
        return 0;
    }
    i32::from_le_bytes(b[0..4].try_into().unwrap())
}

fn read_f64(api: &common::Api, sel: &str, offset: usize) -> f64 {
    let b = read_bytes(api, sel, offset, 8);
    if b.len() < 8 {
        return 0.0;
    }
    f64::from_le_bytes(b[0..8].try_into().unwrap())
}

fn read_bytes(api: &common::Api, sel: &str, offset: usize, length: usize) -> Vec<u8> {
    let r = api.op(
        "read_bytes",
        json!({ "instance_selector": sel, "offset": offset, "length": length }),
    );
    if !r.ok {
        println!("read_bytes({sel}, {offset:#x}, {length}) failed: {:?}", r.error);
        return Vec::new();
    }
    let hex = r.result["bytes_hex"].as_str().unwrap_or("");
    (0..hex.len() / 2)
        .filter_map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok())
        .collect()
}
