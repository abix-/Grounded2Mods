//! Research test for NPC spawning (research.md section 25).
//!
//! BP_AISpawningVolume_C is an invisible box with an authored
//! list of what to spawn: a TArray of S_AISpawner at +0x2D8,
//! each entry {AICharacter class ptr at 0x00, SpawnCount i32 at
//! 0x08}, stride 0x10.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_spawners -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use modforge::client;

type Api = common::Api;

// BP_DwarfSpawn_C: the game's NPC spawn point (the placed
// instance is named BP_AISpawnPoint_C_0). One NPC class + count
// per point. Offsets from the object dump.
const SPAWN_POINT_CLASS: &str = "BP_DwarfSpawn_C";
const ENABLE_OFFSET: u64 = 0x2C8;
const SPAWN_AI_CLASS_OFFSET: u64 = 0x2D0;
const SPAWN_AI_COUNT_OFFSET: u64 = 0x2D8;
const SPAWN_TIME_OFFSET: u64 = 0x2E0;
const RESPAWN_OFFSET: u64 = 0x2F0;
const RESPAWN_TIME_OFFSET: u64 = 0x2F8;
const SPAWNED_AI_OFFSET: u64 = 0x308;
const USE_PROX_OFFSET: u64 = 0x319;
const ACTIVATION_RANGE_OFFSET: u64 = 0x320;
const PLAYER_IN_AREA_OFFSET: u64 = 0x338;
const CURRENT_SPAWNED_OFFSET: u64 = 0x33C;
const LEVEL_NAME_OFFSET: u64 = 0x3A0;
const LOCATION_OFFSET: u64 = 0x3A8;

/// UObject::NamePrivate in UE5: index at +0x18, number at +0x1C.
fn object_name(api: &Api, obj_addr: u64) -> String {
    let bytes = client::read_bytes(api, obj_addr, 0x18, 8);
    if bytes.len() < 8 {
        return format!("(?unreadable {obj_addr:#x})");
    }
    let idx = client::from_le_u32(&bytes, 0);
    let num = client::from_le_u32(&bytes, 4);
    client::fname_from_parts(api, idx, num)
        .unwrap_or_else(|| format!("(?idx={idx:#x})"))
}

fn read_u8(api: &Api, addr: u64, off: u64) -> u8 {
    client::read_bytes(api, addr, off, 1).first().copied().unwrap_or(255)
}

fn read_i32_at(api: &Api, addr: u64, off: u64) -> i32 {
    let b = client::read_bytes(api, addr, off, 4);
    if b.len() == 4 { client::from_le_i32(&b, 0) } else { -1 }
}

fn read_f64_at(api: &Api, addr: u64, off: u64) -> f64 {
    let b = client::read_bytes(api, addr, off, 8);
    if b.len() == 8 { client::from_le_f64(&b, 0) } else { -1.0 }
}

fn read_fname_at(api: &Api, addr: u64, off: u64) -> String {
    let b = client::read_bytes(api, addr, off, 8);
    if b.len() != 8 {
        return String::new();
    }
    client::fname_from_parts(api, client::from_le_u32(&b, 0), client::from_le_u32(&b, 4))
        .unwrap_or_default()
}

fn array_num(api: &Api, addr: u64, offset: u64) -> i32 {
    client::read_tarray_header(api, addr, offset)
        .map(|h| h.num)
        .unwrap_or(-1)
}

/// Dump every live NPC spawn point: its NPC class, count, and
/// settings.
#[test]
fn dump_spawn_points() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }

    let points = client::walk_class_chain_instances(&api, SPAWN_POINT_CLASS, 400);
    println!("{} spawn point(s) live\n", points.len());

    for p in &points {
        let class_ptr_b = client::read_bytes(&api, p.addr, SPAWN_AI_CLASS_OFFSET, 8);
        let class_ptr = if class_ptr_b.len() == 8 {
            client::from_le_u64(&class_ptr_b, 0)
        } else {
            0
        };
        let ai_class = if class_ptr != 0 {
            object_name(&api, class_ptr)
        } else {
            String::from("(null)")
        };
        let loc = client::read_bytes(&api, p.addr, LOCATION_OFFSET, 24);
        let (x, y, z) = if loc.len() == 24 {
            (
                client::from_le_f64(&loc, 0),
                client::from_le_f64(&loc, 8),
                client::from_le_f64(&loc, 16),
            )
        } else {
            (0.0, 0.0, 0.0)
        };

        println!("=== {} @ {} ===", p.name, p.addr_selector);
        println!("  full: {}", p.full_name);
        println!(
            "  spawn {} x {ai_class} (class {class_ptr:#x})",
            read_i32_at(&api, p.addr, SPAWN_AI_COUNT_OFFSET),
        );
        println!(
            "  enable={} respawn={} respawn_time={} spawn_time={}",
            read_u8(&api, p.addr, ENABLE_OFFSET),
            read_u8(&api, p.addr, RESPAWN_OFFSET),
            read_f64_at(&api, p.addr, RESPAWN_TIME_OFFSET),
            read_f64_at(&api, p.addr, SPAWN_TIME_OFFSET),
        );
        println!(
            "  use_prox={} range={} in_area={} current_spawned={} spawned_num={}",
            read_u8(&api, p.addr, USE_PROX_OFFSET),
            read_f64_at(&api, p.addr, ACTIVATION_RANGE_OFFSET),
            read_u8(&api, p.addr, PLAYER_IN_AREA_OFFSET),
            read_i32_at(&api, p.addr, CURRENT_SPAWNED_OFFSET),
            array_num(&api, p.addr, SPAWNED_AI_OFFSET),
        );
        println!(
            "  level={} loc=({x:.0}, {y:.0}, {z:.0})\n",
            read_fname_at(&api, p.addr, LEVEL_NAME_OFFSET),
        );
    }
}

fn write_bytes_op(api: &Api, sel: &str, offset: u64, data: &[u8]) -> bool {
    let r = api.op(
        "write_bytes",
        serde_json::json!({"instance_selector": sel, "offset": offset,
               "bytes_hex": hex::encode(data)}),
    );
    r.ok
}

/// Raise the spawn point's NPC count from 1 to 5 and turn on
/// respawn. The game's own Blueprint logic does the spawning;
/// this only rewrites the authored numbers.
#[test]
#[ignore = "writes to live game"]
fn set_spawn_point_more() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let points = client::walk_class_chain_instances(&api, SPAWN_POINT_CLASS, 8);
    let Some(p) = points.first() else {
        println!("no spawn point live");
        return;
    };
    println!(
        "before: count={} respawn={} current_spawned={}",
        read_i32_at(&api, p.addr, SPAWN_AI_COUNT_OFFSET),
        read_u8(&api, p.addr, RESPAWN_OFFSET),
        read_i32_at(&api, p.addr, CURRENT_SPAWNED_OFFSET),
    );
    let sel = &p.addr_selector;
    assert!(write_bytes_op(&api, sel, SPAWN_AI_COUNT_OFFSET, &5i32.to_le_bytes()));
    assert!(write_bytes_op(&api, sel, RESPAWN_OFFSET, &[1u8]));
    assert!(write_bytes_op(&api, sel, RESPAWN_TIME_OFFSET, &5.0f64.to_le_bytes()));
    println!(
        "after: count={} respawn={} respawn_time={}",
        read_i32_at(&api, p.addr, SPAWN_AI_COUNT_OFFSET),
        read_u8(&api, p.addr, RESPAWN_OFFSET),
        read_f64_at(&api, p.addr, RESPAWN_TIME_OFFSET),
    );
}

/// Swap the spawn point's NPC class to the class of a live NPC
/// (the first non-tamed one found), so the point spawns that
/// enemy instead.
#[test]
#[ignore = "writes to live game"]
fn set_spawn_point_entity() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let points = client::walk_class_chain_instances(&api, SPAWN_POINT_CLASS, 8);
    let Some(p) = points.first() else {
        println!("no spawn point live");
        return;
    };
    let npcs = client::walk_class_chain_instances(&api, "BP_MasterAICharacter_C", 64);
    let Some(donor) = npcs.iter().find(|n| !n.name.contains("Tamed")) else {
        println!("no donor NPC live");
        return;
    };
    // UObject::ClassPrivate at +0x10.
    let class_b = client::read_bytes(&api, donor.addr, 0x10, 8);
    assert_eq!(class_b.len(), 8, "could not read donor class ptr");
    let class_ptr = client::from_le_u64(&class_b, 0);
    println!(
        "donor {} class {} @ {class_ptr:#x}",
        donor.name,
        object_name(&api, class_ptr),
    );
    assert!(write_bytes_op(&api, &p.addr_selector, SPAWN_AI_CLASS_OFFSET, &class_ptr.to_le_bytes()));
    let after = client::read_bytes(&api, p.addr, SPAWN_AI_CLASS_OFFSET, 8);
    println!(
        "spawn point now spawns {}",
        object_name(&api, client::from_le_u64(&after, 0)),
    );
}

/// Count live NPCs and where they live (which level owns them).
/// Answers whether NPCs come from spawn points or are placed
/// directly in the level tiles.
#[test]
fn dump_live_npcs() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let npcs = client::walk_class_chain_instances(&api, "BP_MasterAICharacter_C", 400);
    println!("{} live NPC(s) under BP_MasterAICharacter_C\n", npcs.len());
    for n in &npcs {
        println!("  {}", n.full_name);
    }
}
