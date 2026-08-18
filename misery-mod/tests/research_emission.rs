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
use common::{api_or_skip, offsets_live, show};
use modforge::client;
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
    let Some(inst) = client::find_live_instance(&api, GLOBAL_MANAGER) else {
        println!("no live {GLOBAL_MANAGER}");
        return;
    };
    println!("selector: {}  ({})", inst.addr_selector, inst.full_name);

    for (name, off, len) in [
        ("EmissionsCount", EMISSIONS_COUNT, 4u64),
        ("TimeUntilEmmision", TIME_UNTIL_EMMISION, 8),
        ("FreezeTimer?", FREEZE_TIMER, 1),
        ("FirstEmissionOffset", FIRST_EMISSION_OFFSET, 8),
        ("EmissionRandomDeviation", EMISSION_RANDOM_DEVIATION, 8),
    ] {
        let bytes = client::read_bytes(&api, inst.addr, off, len);
        if bytes.is_empty() {
            continue;
        }
        let decoded = match len {
            8 => format!("{}", client::from_le_f64(&bytes, 0)),
            4 => format!("{}", client::from_le_i32(&bytes, 0)),
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
    let Some(inst) = client::find_live_instance(&api, GLOBAL_MANAGER) else {
        println!("no live {GLOBAL_MANAGER}");
        return;
    };
    let addr = inst.addr;

    println!("sampling TimeUntilEmmision every 2s, 6 times");
    let start = std::time::Instant::now();
    let mut prev: Option<(f64, f64)> = None;
    for i in 0..6 {
        let secs = start.elapsed().as_secs_f64();
        let b = client::read_bytes(&api, addr, TIME_UNTIL_EMMISION, 8);
        if b.len() >= 8 {
            let v = client::from_le_f64(&b, 0);
            let rate = prev
                .map(|(pt, pv)| format!("  ({:+.3}/s)", (v - pv) / (secs - pt)))
                .unwrap_or_default();
            println!("  t={secs:6.2}s  TimeUntilEmmision = {v}{rate}");
            prev = Some((secs, v));
        } else {
            println!("  t={secs:6.2}s  unreadable");
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
    let Some(inst) = client::find_live_instance(&api, "BP_SGKGameInstance_C") else {
        println!("no live BP_SGKGameInstance_C");
        return;
    };
    let addr = inst.addr;
    println!("selector: {}  ({})", inst.addr_selector, inst.full_name);

    const SETTINGS: u64 = 0x218;
    println!("  DifficultyPreset      +0x210 = {}", client::read_u8(&api, addr, 0x210));
    for (name, rel) in [
        ("ShiningsTimer", 0x00u64),
        ("DayLength", 0x08),
        ("NightLength", 0x10),
        ("WeatherCycleDuration", 0x18),
    ] {
        let v = client::read_f64(&api, addr, SETTINGS + rel);
        println!("  {name:<21} +0x{:03X} = {v}", SETTINGS + rel);
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
    let instances = client::walk_class_instances(&api, "BP_WorldGeneration_Base_C", 100);
    for inst in &instances {
        let refresh = client::read_i32(&api, inst.addr, 0x2B8);
        let past = client::read_i32(&api, inst.addr, 0x2F8);
        println!("  EmissionCountForRefresh={refresh}  EmissionsPast={past}  {}", inst.full_name);
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

    if let Some(gi) = client::find_live_instance(&api, "BP_SGKGameInstance_C") {
        let v = client::read_u8(&api, gi.addr, 0x218 + 0xBB);
        println!("GameplaySettings.RespawnOnEmission = {v}");
    }

    let instances = client::walk_class_instances_with_cdo(&api, "BP_PlayerInventory_C", 100);
    for inst in &instances {
        let v = client::read_u8(&api, inst.addr, 0x133A);
        let kind = if inst.full_name.contains("GEN_VARIABLE") { "template" } else { "live" };
        println!("  [{kind}] RespawnOnEmission = {v}  {}", inst.full_name);
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
    let Some(inst) = client::find_live_instance(&api, GLOBAL_MANAGER) else { return };
    let addr = inst.addr;

    for (name, off, len) in [
        ("CurrentGeneratedLevel", 0x2C8u64, 1u64),
        ("CustomBiomSelected", 0x2F8, 1),
        ("LoadedSave", 0x2F9, 1),
        ("FirstSave", 0x2FA, 1),
        ("CurrentWorldSeed", 0x2BC, 4),
    ] {
        let b = client::read_bytes(&api, addr, off, len);
        if b.is_empty() { continue; }
        let v = if len == 4 {
            format!("{}", client::from_le_i32(&b, 0))
        } else {
            format!("{}", b.first().copied().unwrap_or(0))
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

    let r = api.op("read_bytes",
        json!({"instance_selector": "first_class:BP_GlobalManager_C",
               "offset": 0x2B0, "length": 8}));
    println!("first_class read: ok={} err={:?}", r.ok, r.error);

    let Some(door) = client::find_live_instance(&api, "BP_ExpeditionDoor_C") else {
        println!("no expedition door either");
        return;
    };
    let Some(mgr_ptr) = client::read_component_ptr(&api, door.addr, 0x448) else {
        println!("door's GlobalManager pointer is null");
        return;
    };
    println!("door {} -> GlobalManager = 0x{mgr_ptr:X}", door.addr_selector);
    println!("TimeUntilEmmision = {}", client::read_f64(&api, mgr_ptr, 0x2B0));
    println!("EmissionsCount    = {}", client::read_i32(&api, mgr_ptr, 0x2A8));
    println!("FreezeTimer?      = {}", client::read_u8(&api, mgr_ptr, 0x2B8));
    println!("USE THIS SELECTOR: addr:0x{mgr_ptr:X}");
}
