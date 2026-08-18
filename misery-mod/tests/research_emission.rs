//! Step 2 of docs/research.md: read the expedition clock
//! live.
//!
//! The dump says `TimeUntilEmmision` is a Double at offset 0x2B0
//! on `BP_GlobalManager_C`. That is a name and a layout, not
//! proof. These probes find the live instance, read the value,
//! and sample it over time so the direction, the rate and the
//! unit come from observation.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_emission -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, as_f64, as_i32, first_instance, offsets_live, read_bytes,
              selector_of, show};
use serde_json::json;

/// Property offsets on BP_GlobalManager_C, from the object dump
/// (docs/research.md section 8.1).
const EMISSIONS_COUNT: u64 = 0x2A8;
const TIME_UNTIL_EMMISION: u64 = 0x2B0;
const FREEZE_TIMER: u64 = 0x2B8;
const FIRST_EMISSION_OFFSET: u64 = 0x300;
const EMISSION_RANDOM_DEVIATION: u64 = 0x308;

const GLOBAL_MANAGER: &str = "BP_GlobalManager_C";

#[test]
fn control_plane_answers() {
    let Some(api) = api_or_skip() else { return };

    let ops = api.op("list_ops", json!({}));
    show("list_ops", &ops);

    let known = offsets_live(&api);
    println!("offsets_known = {known}");
    assert!(
        known,
        "platform offsets are not live; press Ctrl+R in game to hot-reload, or restart"
    );
}

/// Check the hand-computed offsets in `lib.rs::STEAM` against
/// patternsleuth's own resolvers, which scan the host image and
/// report image-relative offsets. Subtracting an image base by
/// hand is exactly the kind of arithmetic that should be checked
/// by the tool that does it properly.
#[test]
fn resolve_offsets_against_config() {
    let Some(api) = api_or_skip() else { return };
    let r = api.op("resolve_offsets", json!({}));
    show("resolve_offsets", &r);
}

/// `walk_class` turned out to be a class-layout walker, not an
/// instance finder ("Walk a UClass property chain and return the
/// named fields"). Instances come from selectors instead. This
/// prints the selector catalog and tries the likely ones.
#[test]
fn find_global_manager() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live yet");
        return;
    }

    let sels = api.op("list_selectors", json!({}));
    show("list_selectors", &sels);

    println!("=== class layout ===");
    let layout = api.op("walk_class", json!({"class": GLOBAL_MANAGER}));
    show("walk_class", &layout);

    println!("=== read via first_class selector ===");
    let r = api.op(
        "read_bytes",
        json!({"selector": format!("first_class:{GLOBAL_MANAGER}"),
               "offset": TIME_UNTIL_EMMISION, "length": 8}),
    );
    show("first_class read", &r);
}

/// Read every emission field once. Proves the offsets point at
/// plausible values before anything is sampled over time.
#[test]
fn read_emission_fields() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live yet");
        return;
    }
    let Some(inst) = first_instance(&api, GLOBAL_MANAGER) else {
        println!("no live {GLOBAL_MANAGER}");
        return;
    };
    let Some(sel) = selector_of(&inst) else {
        println!("instance has no selector: {inst}");
        return;
    };
    println!("selector: {sel}  ({})", inst["full_name"]);

    for (name, off, len) in [
        ("EmissionsCount", EMISSIONS_COUNT, 4u64),
        ("TimeUntilEmmision", TIME_UNTIL_EMMISION, 8),
        ("FreezeTimer?", FREEZE_TIMER, 1),
        ("FirstEmissionOffset", FIRST_EMISSION_OFFSET, 8),
        ("EmissionRandomDeviation", EMISSION_RANDOM_DEVIATION, 8),
    ] {
        let Some(bytes) = read_bytes(&api, &sel, off, len) else {
            continue;
        };
        let decoded = match len {
            8 => format!("{:?}", as_f64(&bytes)),
            4 => format!("{:?}", as_i32(&bytes)),
            _ => format!("{}", bytes.first().copied().unwrap_or(0)),
        };
        println!("  {name:<24} +0x{off:03X} = {decoded}  raw={bytes:02X?}");
    }
}

/// Sample the countdown against wall-clock seconds. This is the
/// probe that turns a name into the clock: if it falls by ~N per
/// second, the unit and rate are known.
#[test]
fn sample_countdown() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live yet");
        return;
    }
    let Some(inst) = first_instance(&api, GLOBAL_MANAGER) else {
        println!("no live {GLOBAL_MANAGER}");
        return;
    };
    let Some(sel) = selector_of(&inst) else {
        println!("instance has no selector");
        return;
    };

    println!("sampling TimeUntilEmmision every 2s, 6 times");
    let start = std::time::Instant::now();
    let mut prev: Option<(f64, f64)> = None;
    for i in 0..6 {
        let secs = start.elapsed().as_secs_f64();
        match read_bytes(&api, &sel, TIME_UNTIL_EMMISION, 8).and_then(|b| as_f64(&b)) {
            Some(v) => {
                let rate = prev
                    .map(|(pt, pv)| format!("  ({:+.3}/s)", (v - pv) / (secs - pt)))
                    .unwrap_or_default();
                println!("  t={secs:6.2}s  TimeUntilEmmision = {v}{rate}");
                prev = Some((secs, v));
            }
            None => println!("  t={secs:6.2}s  unreadable"),
        }
        if i < 5 {
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    }
}

/// The configured interval lives on the GameInstance:
/// `DifficultyPreset` at 0x210 and the `S_GameplaySettings`
/// struct at 0x218, whose first field (+0x00) is `ShiningsTimer`.
/// Step 3 hinges on whether the countdown's reset value equals
/// this number.
#[test]
fn read_gameplay_settings() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let Some(inst) = first_instance(&api, "BP_SGKGameInstance_C") else {
        println!("no live BP_SGKGameInstance_C");
        return;
    };
    let Some(sel) = selector_of(&inst) else { return };
    println!("selector: {sel}  ({})", inst["full_name"]);

    const SETTINGS: u64 = 0x218;
    if let Some(b) = read_bytes(&api, &sel, 0x210, 1) {
        println!("  DifficultyPreset      +0x210 = {:?}", b.first());
    }
    // S_GameplaySettings field order from the object dump
    // (docs/research.md section 8.2).
    for (name, rel) in [
        ("ShiningsTimer", 0x00u64),
        ("DayLength", 0x08),
        ("NightLength", 0x10),
        ("WeatherCycleDuration", 0x18),
    ] {
        if let Some(v) = read_bytes(&api, &sel, SETTINGS + rel, 8).and_then(|b| as_f64(&b)) {
            println!("  {name:<21} +0x{:03X} = {v}", SETTINGS + rel);
        }
    }
}

/// What a shining actually does, part 1: world regeneration.
/// `BP_WorldGeneration_Base_C` counts `EmissionsPast` (+0x2F8)
/// against `EmissionCountForRefresh` (+0x2B8), so emissions are
/// what drive the world refreshing.
#[test]
fn read_world_regeneration() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    // Four instances, subclassed (factory, and others). Read
    // them all: the first one read 0/0 and that may be true only
    // of that generator.
    let r = api.op("walk_class", json!({"class": "BP_WorldGeneration_Base_C"}));
    let empty = vec![];
    for inst in r.result["instances"].as_array().unwrap_or(&empty) {
        let name = inst["full_name"].as_str().unwrap_or("?");
        let Some(sel) = selector_of(inst) else { continue };
        let refresh = read_bytes(&api, &sel, 0x2B8, 4).and_then(|b| as_i32(&b));
        let past = read_bytes(&api, &sel, 0x2F8, 4).and_then(|b| as_i32(&b));
        println!("  EmissionCountForRefresh={refresh:?}  EmissionsPast={past:?}  {name}");
    }
}

/// Is respawn-on-shining switched on for this save? The flag
/// exists in two places: the authored gameplay settings
/// (S_GameplaySettings +0xBB, so GameInstance +0x218+0xBB) and
/// the live player inventory component (+0x133A).
#[test]
fn read_respawn_on_emission() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }

    if let Some(gi) = first_instance(&api, "BP_SGKGameInstance_C")
        && let Some(sel) = selector_of(&gi)
        && let Some(b) = read_bytes(&api, &sel, 0x218 + 0xBB, 1)
    {
        println!("GameplaySettings.RespawnOnEmission = {:?}", b.first());
    }

    // walk_class returns the archetype (a GEN_VARIABLE template)
    // alongside the real component, and the template's flags mean
    // nothing. Read every instance and label them.
    let r = api.op("walk_class", json!({"class": "BP_PlayerInventory_C"}));
    let empty = vec![];
    for inst in r.result["instances"].as_array().unwrap_or(&empty) {
        let name = inst["full_name"].as_str().unwrap_or("?");
        let Some(sel) = selector_of(inst) else { continue };
        let v = read_bytes(&api, &sel, 0x133A, 1).and_then(|b| b.first().copied());
        let kind = if name.contains("GEN_VARIABLE") { "template" } else { "live" };
        println!("  [{kind}] RespawnOnEmission = {v:?}  {name}");
    }
}

/// Which biome is the expedition currently running, and did the
/// player pick it? `BP_GlobalManager_C` holds
/// `CurrentGeneratedLevel` (Byte, 0x2C8) and `CustomBiomSelected`
/// (Bool, 0x2F8), and has GenerateBiom / GenerateCustomBiom /
/// SelectRandomBiom to drive it.
#[test]
fn read_current_biome() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let Some(inst) = first_instance(&api, GLOBAL_MANAGER) else { return };
    let Some(sel) = selector_of(&inst) else { return };

    for (name, off, len) in [
        ("CurrentGeneratedLevel", 0x2C8u64, 1u64),
        ("CustomBiomSelected", 0x2F8, 1),
        ("LoadedSave", 0x2F9, 1),
        ("FirstSave", 0x2FA, 1),
        ("CurrentWorldSeed", 0x2BC, 4),
    ] {
        let Some(b) = read_bytes(&api, &sel, off, len) else { continue };
        let v = if len == 4 {
            format!("{:?}", as_i32(&b))
        } else {
            format!("{:?}", b.first())
        };
        println!("  {name:<22} +0x{off:03X} = {v}");
    }
}

/// walk_class stopped finding BP_GlobalManager_C while the world
/// was still running. Find out why: does the class resolve, does
/// its CDO exist, and can the actor be reached another way?
/// BP_ExpeditionDoor_C holds a GlobalManager reference at +0x448.
#[test]
fn diagnose_missing_manager() {
    let Some(api) = api_or_skip() else { return };
    println!("offsets_known = {}", offsets_live(&api));

    for class in ["BP_GlobalManager_C", "BP_ExpeditionDoor_C",
                  "BP_PaneliWorldGeneration_C", "BP_SGKGameInstance_C"] {
        let w = api.op("walk_class", json!({"class": class}));
        println!("walk {class:<28} ok={} total={} err={:?}",
            w.ok, w.result["total"], w.error);
    }

    // NEVER read an actor offset off `singleton:` (the class
    // default object). A CDO is not laid out like a live actor,
    // so the read lands on invalid memory. Doing exactly that
    // crashed the game on 2026-08-14, mid-session. Only
    // `first_class:` / `addr:` selectors point at real actors.
    let r = api.op("read_bytes",
        json!({"instance_selector": "first_class:BP_GlobalManager_C",
               "offset": 0x2B0, "length": 8}));
    println!("first_class read: ok={} err={:?}", r.ok, r.error);

    let Some(door) = first_instance(&api, "BP_ExpeditionDoor_C") else {
        println!("no expedition door either");
        return;
    };
    let Some(dsel) = selector_of(&door) else { return };
    let Some(b) = read_bytes(&api, &dsel, 0x448, 8) else { return };
    let addr = u64::from_le_bytes(b[..8].try_into().unwrap_or_default());
    println!("door {dsel} -> GlobalManager = 0x{addr:X}");
    if addr == 0 {
        println!("door's GlobalManager pointer is null");
        return;
    }
    let msel = format!("addr:0x{addr:X}");
    println!("TimeUntilEmmision = {:?}",
        read_bytes(&api, &msel, 0x2B0, 8).and_then(|b| as_f64(&b)));
    println!("EmissionsCount    = {:?}",
        read_bytes(&api, &msel, 0x2A8, 4).and_then(|b| as_i32(&b)));
    println!("FreezeTimer?      = {:?}",
        read_bytes(&api, &msel, 0x2B8, 1).and_then(|b| b.first().copied()));
    println!("USE THIS SELECTOR: {msel}");
}
