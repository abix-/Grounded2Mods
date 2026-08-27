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


const LEVELS_POOL: u64 = 0x2C8;
const LEVELS_REFRESHED_POOL: u64 = 0x2D8;
const POOL_STRIDE: u64 = 0x28;
const POOL_ASSET_NAME: usize = 0x10;

/// Read a generator's pool as raw elements plus asset names.
fn pool_entries(api: &Api, gen_addr: u64) -> Option<(u64, Vec<(String, Vec<u8>)>)> {
    pool_entries_at(api, gen_addr, LEVELS_POOL)
}

fn pool_entries_at(
    api: &Api,
    gen_addr: u64,
    offset: u64,
) -> Option<(u64, Vec<(String, Vec<u8>)>)> {
    let hdr = client::read_tarray_header(api, gen_addr, offset)?;
    if hdr.num <= 0 || hdr.num > 64 {
        return None;
    }
    let data = client::read_bytes(api, hdr.ptr, 0, (hdr.num as u64) * POOL_STRIDE);
    if data.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for i in 0..hdr.num as usize {
        let base = i * POOL_STRIDE as usize;
        let elem = data[base..base + POOL_STRIDE as usize].to_vec();
        let idx = client::from_le_u32(&elem, POOL_ASSET_NAME);
        let num = client::from_le_u32(&elem, POOL_ASSET_NAME + 4);
        let name = client::fname_from_parts(api, idx, num).unwrap_or_default();
        out.push((name, elem));
    }
    Some((hdr.ptr, out))
}

/// Pool swap: copy a Meadows square's pool element over a
/// Paneli slot, force a Paneli world, and verify the foreign
/// square streams into the Paneli grid (y 7..9). The write is
/// runtime-only and resets on save reload.
#[test]
#[ignore = "rebuilds the expedition world"]
fn pool_swap_meadows_into_paneli() {
    const DONOR: &str = "L_VehCemetry_Bridge";
    const VICTIM: &str = "L_Town01";

    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let gens = client::walk_class_chain_instances(&api, "BP_WorldGeneration_Base_C", 8);
    let meadows = gens.iter().find(|g| g.name.contains("Meadows")).expect("no Meadows generator");
    let paneli = gens.iter().find(|g| g.name.contains("Paneli")).expect("no Paneli generator");

    let (_, m_pool) = pool_entries(&api, meadows.addr).expect("meadows pool unreadable");
    let donor = m_pool
        .iter()
        .find(|(n, _)| n == DONOR)
        .expect("donor square not in Meadows pool");
    let (p_ptr, p_pool) = pool_entries(&api, paneli.addr).expect("paneli pool unreadable");
    if let Some(victim_idx) = p_pool.iter().position(|(n, _)| n == VICTIM) {
        println!("writing {DONOR} over Paneli slot {victim_idx} ({VICTIM})");
        let sel = format!("addr:0x{p_ptr:x}");
        assert!(modforge::client::write_bytes_at(&api, &sel, victim_idx as u64 * POOL_STRIDE, &donor.1));
        let (_, p_after) = pool_entries(&api, paneli.addr).expect("paneli pool unreadable");
        assert_eq!(p_after[victim_idx].0, DONOR, "write did not land");
    } else {
        // Rerun: the swap is already in place from a previous run.
        assert!(
            p_pool.iter().any(|(n, _)| n == DONOR),
            "neither victim nor donor in Paneli pool; unexpected pool state"
        );
        println!("swap already in place, rerolling the world");
    }
    let (_, p_now) = pool_entries(&api, paneli.addr).expect("paneli pool unreadable");
    println!("paneli pool now:");
    for (i, (n, _)) in p_now.iter().enumerate() {
        println!("  {i:>2}. {n}");
    }

    let m = manager(&api).expect("no global manager");
    println!("forcing Paneli world (GenerateCustomBiom(3))");
    api.call_ufunction("BP_GlobalManager_C", "GenerateCustomBiom", &m.addr_selector, &[3u8])
        .expect("GenerateCustomBiom failed");

    // Streaming plus NPC creation takes a while.
    std::thread::sleep(Duration::from_secs(20));

    let mut squares: std::collections::HashSet<String> = std::collections::HashSet::new();
    for n in client::walk_class_chain_instances(&api, "BP_MasterAICharacter_C", 400) {
        if let Some(path) = n.full_name.split(' ').nth(1) {
            if let Some(sq) = path.split(".PersistentLevel").next() {
                squares.insert(sq.rsplit('/').next().unwrap_or(sq).to_string());
            }
        }
    }
    println!("squares with NPCs after regen:");
    for s in &squares {
        println!("  {s}");
    }
    let hit = squares.iter().find(|s| s.contains(DONOR));
    assert!(
        hit.is_some(),
        "foreign square {DONOR} not found in the generated world \
         (it may simply not have been rolled; rerun to reroll)"
    );
    println!("FOREIGN SQUARE PLACED: {}", hit.unwrap());
}

/// Mixed-pool area: fill all nine Paneli slots with a curated
/// blend of Meadows and Town squares (both 12000-unit tiles)
/// and generate the world. Sources are collected from both
/// pools before writing, so reruns work.
#[test]
#[ignore = "rebuilds the expedition world"]
fn mixed_pool_area() {
    const BLEND: [&str; 9] = [
        "L_Kolhoz01",
        "L_VehCemetry_Bridge",
        "L_River_LoggingCamp",
        "L_TownSwamp01",
        "L_Village_Dwarf_Hole",
        "L_Anomaly_House",
        "L_BombCrater",
        "L_Forest02",
        "L_Town_Anomaly01",
    ];

    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let gens = client::walk_class_chain_instances(&api, "BP_WorldGeneration_Base_C", 8);
    let meadows = gens.iter().find(|g| g.name.contains("Meadows")).expect("no Meadows generator");
    let paneli = gens.iter().find(|g| g.name.contains("Paneli")).expect("no Paneli generator");

    // Collect source elements from both pools before writing.
    let (_, m_pool) = pool_entries(&api, meadows.addr).expect("meadows pool unreadable");
    let (p_ptr, p_pool) = pool_entries(&api, paneli.addr).expect("paneli pool unreadable");
    let mut sources: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
    for (n, e) in m_pool.iter().chain(p_pool.iter()) {
        sources.entry(n.clone()).or_insert_with(|| e.clone());
    }

    let sel = format!("addr:0x{p_ptr:x}");
    for (slot, name) in BLEND.iter().enumerate() {
        let elem = sources
            .get(*name)
            .unwrap_or_else(|| panic!("{name} not found in either pool"));
        assert!(
            modforge::client::write_bytes_at(&api, &sel, slot as u64 * POOL_STRIDE, elem),
            "write failed for slot {slot}"
        );
    }
    let (_, p_after) = pool_entries(&api, paneli.addr).expect("paneli pool unreadable");
    println!("paneli pool is now the blend:");
    for (i, (n, _)) in p_after.iter().enumerate() {
        println!("  {i:>2}. {n}");
        assert_eq!(n, BLEND[i], "slot {i} did not take");
    }

    let m = manager(&api).expect("no global manager");
    println!("generating the mixed world (GenerateCustomBiom(3))");
    api.call_ufunction("BP_GlobalManager_C", "GenerateCustomBiom", &m.addr_selector, &[3u8])
        .expect("GenerateCustomBiom failed");
    std::thread::sleep(Duration::from_secs(20));

    let mut board: Vec<String> = Vec::new();
    for n in client::walk_class_chain_instances(&api, "BP_MasterAICharacter_C", 400) {
        if let Some(path) = n.full_name.split(' ').nth(1) {
            if let Some(sq) = path.split(".PersistentLevel").next() {
                let short = sq.rsplit('/').next().unwrap_or(sq).to_string();
                if !board.contains(&short) {
                    board.push(short);
                }
            }
        }
    }
    board.sort();
    println!("the generated board (squares with NPCs):");
    for s in &board {
        println!("  {s}");
    }
    let meadows_squares = ["L_Kolhoz01", "L_VehCemetry_Bridge", "L_River_LoggingCamp",
        "L_Village_Dwarf_Hole", "L_BombCrater", "L_Forest02"];
    let town_squares = ["L_TownSwamp01", "L_Anomaly_House", "L_Town_Anomaly01"];
    let m_hit = board.iter().filter(|s| meadows_squares.iter().any(|q| s.contains(q))).count();
    let t_hit = board.iter().filter(|s| town_squares.iter().any(|q| s.contains(q))).count();
    println!("meadows squares on the board: {m_hit}, town squares: {t_hit}");
    assert!(
        m_hit > 0 && t_hit > 0,
        "board is not mixed (meadows={m_hit}, town={t_hit}); reroll"
    );
}

/// Tile-size mismatch probe, oversized direction: a 16500-unit
/// Factory square written into the 12000-unit Paneli grid.
/// Expected: the square streams but overlaps its neighbors by
/// 4500 units. Auto-rerolls until the square is placed, then
/// restores the pool slot (the generated world keeps the
/// oversized square for inspection).
#[test]
#[ignore = "rebuilds the expedition world"]
fn size_mismatch_probe() {
    const DONOR: &str = "L_CementFactory_Art";

    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let gens = client::walk_class_chain_instances(&api, "BP_WorldGeneration_Base_C", 8);
    let factory = gens.iter().find(|g| g.name.contains("Factory")).expect("no Factory generator");
    let paneli = gens.iter().find(|g| g.name.contains("Paneli")).expect("no Paneli generator");

    let (_, f_pool) = pool_entries(&api, factory.addr).expect("factory pool unreadable");
    let donor = f_pool
        .iter()
        .find(|(n, _)| n == DONOR)
        .expect("donor square not in Factory pool");
    let (p_ptr, p_pool) = pool_entries(&api, paneli.addr).expect("paneli pool unreadable");
    let original = p_pool[0].1.clone();
    println!("writing {DONOR} (16500) over Paneli slot 0 ({})", p_pool[0].0);
    let sel = format!("addr:0x{p_ptr:x}");
    assert!(modforge::client::write_bytes_at(&api, &sel, 0, &donor.1));

    let m = manager(&api).expect("no global manager");
    let mut placed: Option<String> = None;
    for attempt in 1..=4 {
        println!("attempt {attempt}: generating Paneli world");
        api.call_ufunction("BP_GlobalManager_C", "GenerateCustomBiom", &m.addr_selector, &[3u8])
            .expect("GenerateCustomBiom failed");
        std::thread::sleep(Duration::from_secs(20));
        for n in client::walk_class_chain_instances(&api, "BP_MasterAICharacter_C", 400) {
            if n.full_name.contains(DONOR) {
                if let Some(path) = n.full_name.split(' ').nth(1) {
                    if let Some(sq) = path.split(".PersistentLevel").next() {
                        placed = Some(sq.rsplit('/').next().unwrap_or(sq).to_string());
                    }
                }
            }
        }
        if placed.is_some() {
            break;
        }
    }

    // Restore the pool slot regardless of outcome.
    assert!(modforge::client::write_bytes_at(&api, &sel, 0, &original), "slot restore failed");
    println!("pool slot 0 restored");

    match &placed {
        Some(sq) => println!(
            "OVERSIZED SQUARE PLACED: {sq}. Inspect in-game: expect \
             4500 units of overlap into the neighboring cells."
        ),
        None => println!("square never rolled in 4 attempts; rerun for more rolls"),
    }
    assert!(placed.is_some(), "no placement in 4 attempts");
}

/// Factory mystery: GenerateCustomBiom(1) does nothing. Try
/// the other path: write CurrentGeneratedLevel = 1 directly,
/// then call the parameterless GenerateBiom.
#[test]
#[ignore = "rebuilds the expedition world"]
fn factory_via_generate_biom() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let m = manager(&api).expect("no global manager");
    print_state(&api, "before");
    assert!(modforge::client::write_bytes_at(&api, &m.addr_selector, CURRENT_GENERATED_LEVEL, &[1u8]));
    println!("wrote CurrentGeneratedLevel=1, calling GenerateBiom()");
    api.call_ufunction("BP_GlobalManager_C", "GenerateBiom", &m.addr_selector, &[])
        .expect("GenerateBiom failed");
    for i in 0..4 {
        std::thread::sleep(Duration::from_secs(5));
        println!("--- {}s ---", (i + 1) * 5);
        print_state(&api, "after");
    }
}

/// Factory mystery, second angle: bypass the manager and call
/// GenerateNewRandomLevels directly on the Factory generator.
#[test]
#[ignore = "rebuilds the expedition world"]
fn factory_via_generator_direct() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let gens = client::walk_class_chain_instances(&api, "BP_WorldGeneration_Base_C", 8);
    let factory = gens.iter().find(|g| g.name.contains("Factory")).expect("no Factory generator");
    print_state(&api, "before");
    println!("calling GenerateNewRandomLevels on {}", factory.name);
    api.call_ufunction(
        "BP_WorldGeneration_Base_C",
        "GenerateNewRandomLevels",
        &factory.addr_selector,
        &[],
    )
    .expect("GenerateNewRandomLevels failed");
    for i in 0..4 {
        std::thread::sleep(Duration::from_secs(5));
        println!("--- {}s ---", (i + 1) * 5);
        print_state(&api, "after");
    }
}

/// Dump BOTH pools per generator: Levels (+0x2C8) and
/// LevelsRefreshed (+0x2D8). Post-refresh worlds contain
/// squares absent from Levels, so LevelsRefreshed is the
/// suspected second source.
#[test]
fn dump_both_pools() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    for g in client::walk_class_chain_instances(&api, "BP_WorldGeneration_Base_C", 8) {
        for (label, off) in [("Levels", LEVELS_POOL), ("LevelsRefreshed", LEVELS_REFRESHED_POOL)] {
            match pool_entries_at(&api, g.addr, off) {
                Some((_, entries)) => {
                    println!("=== {} {label}: {} entries ===", g.name, entries.len());
                    for (i, (n, _)) in entries.iter().enumerate() {
                        println!("  {i:>2}. {n}");
                    }
                }
                None => println!("=== {} {label}: empty or unreadable ===", g.name),
            }
        }
    }
}

/// Where does a square sit in the world? Group live NPCs by
/// their owning square, read each one's location, and compare
/// the square's bounding box with the grid cell parsed from its
/// name times TileSize. Tells us whether decorations can be
/// placed by grid math instead of anchoring on NPCs.
#[test]
fn square_world_bounds() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let mut by_square: std::collections::BTreeMap<String, Vec<(f64, f64, f64)>> =
        std::collections::BTreeMap::new();
    for n in client::walk_class_chain_instances(&api, "BP_MasterAICharacter_C", 400) {
        let Some(path) = n.full_name.split(' ').nth(1) else { continue };
        let Some(sq) = path.split(".PersistentLevel").next() else { continue };
        let short = sq.rsplit('/').next().unwrap_or(sq).to_string();
        if !short.contains('_') {
            continue;
        }
        let parms = vec![0u8; 0x18];
        let Ok((out, _)) =
            api.call_ufunction("Actor", "K2_GetActorLocation", &n.addr_selector, &parms)
        else {
            continue;
        };
        if out.len() < 0x18 {
            continue;
        }
        by_square.entry(short).or_default().push((
            client::from_le_f64(&out, 0x00),
            client::from_le_f64(&out, 0x08),
            client::from_le_f64(&out, 0x10),
        ));
    }

    for (square, pts) in &by_square {
        let min_x = pts.iter().map(|p| p.0).fold(f64::MAX, f64::min);
        let max_x = pts.iter().map(|p| p.0).fold(f64::MIN, f64::max);
        let min_y = pts.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        let max_y = pts.iter().map(|p| p.1).fold(f64::MIN, f64::max);
        let z = pts.iter().map(|p| p.2).sum::<f64>() / pts.len() as f64;
        // Name shape: <worldid>_<cellx>_<celly>.L_Whatever
        let cell: Vec<&str> = square.split('.').next().unwrap_or("").split('_').collect();
        let predicted = if cell.len() == 3 {
            match (cell[1].parse::<f64>(), cell[2].parse::<f64>()) {
                (Ok(cx), Ok(cy)) => format!("cell({cx},{cy}) x12000 = ({}, {})", cx * 12000.0, cy * 12000.0),
                _ => String::from("(unparsed)"),
            }
        } else {
            String::from("(no cell)")
        };
        println!(
            "{square}: {} npc(s) x {min_x:.0}..{max_x:.0} y {min_y:.0}..{max_y:.0} z~{z:.0}",
            pts.len()
        );
        println!("    {predicted}");
    }
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
