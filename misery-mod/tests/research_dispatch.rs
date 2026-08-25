//! Game-thread dispatch verification (src/dispatch.rs).
//!
//! Proves that a job enqueued from the HTTP worker thread runs
//! on the game thread via the ProcessEvent drain site.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_dispatch -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use modforge::client;
use serde_json::json;

/// Compare the CDO's vtable pointer with a live player
/// instance's. If they differ, patching the CDO's vtable can
/// never intercept live calls (Blueprint reinstancing,
/// research.md 22.13).
#[test]
fn vtable_compare() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let cdo = client::find_class_cdo(&api, "BP_SGKMasterCharacter_C");
    let live = client::walk_class_chain_instances(&api, "BP_SGKMasterCharacter_C", 4);
    let read_vtable = |addr: u64| -> u64 {
        let b = client::read_bytes(&api, addr, 0, 8);
        if b.len() == 8 { client::from_le_u64(&b, 0) } else { 0 }
    };
    match (&cdo, live.first()) {
        (Some(c), Some(l)) => {
            let cv = read_vtable(c.addr);
            let lv = read_vtable(l.addr);
            println!("cdo  {} vtable {cv:#x}", c.name);
            println!("live {} vtable {lv:#x}", l.name);
            println!("match: {}", cv == lv);
            // ProcessEvent slot 0x4C (lib.rs PROCESS_EVENT_IDX).
            // If the value points into the game exe, our patch is
            // not in the dispatch path; if it points into the mod
            // DLL, the patch is live but never invoked.
            let slot_addr = lv + 0x4C * 8;
            let slot = client::read_bytes(&api, slot_addr, 0, 8);
            if slot.len() == 8 {
                println!(
                    "slot[0x4C] @ {slot_addr:#x} = {:#x}",
                    client::from_le_u64(&slot, 0)
                );
            }

            // Find the TRUE ProcessEvent index: UE4SS resolves
            // ProcessEvent at startup and logs the address; scan
            // the vtable for it.
            let log = std::fs::read_to_string(
                "C:/Games/Steam/steamapps/common/MISERY/MISERY/Binaries/Win64/ue4ss/UE4SS.log",
            )
            .unwrap_or_default();
            let pe_addr = log
                .lines()
                .filter_map(|l| l.split("ProcessEvent address 0x").nth(1))
                .last()
                .and_then(|h| u64::from_str_radix(h.trim(), 16).ok());
            if let Some(pe) = pe_addr {
                println!("ue4ss ProcessEvent = {pe:#x}");
                let table = client::read_bytes(&api, lv, 0, 0x100 * 8);
                let mut found = false;
                for i in 0..(table.len() / 8) {
                    let v = client::from_le_u64(&table, i * 8);
                    if v == pe {
                        println!("ProcessEvent vtable index = {i:#x}");
                        found = true;
                    }
                    if (0x40..0x60).contains(&i) {
                        println!("  slot[{i:#x}] = {v:#x}");
                    }
                }
                if !found {
                    println!("ProcessEvent not found in first 0x100 actor slots");
                }
                // AActor overrides ProcessEvent, so the actor
                // vtable holds the override, not the base address
                // UE4SS logs. A plain UObject (the GameInstance)
                // keeps the base implementation at the same index.
                let gi = client::walk_class_chain_instances(&api, "BP_SGKGameInstance_C", 2);
                if let Some(g) = gi.first() {
                    let gv = read_vtable(g.addr);
                    println!("gameinstance {} vtable {gv:#x}", g.name);
                    let gtable = client::read_bytes(&api, gv, 0, 0x100 * 8);
                    for i in 0..(gtable.len() / 8) {
                        if client::from_le_u64(&gtable, i * 8) == pe {
                            println!("ProcessEvent vtable index (uobject) = {i:#x}");
                        }
                    }
                } else {
                    println!("no live BP_SGKGameInstance_C");
                }
            } else {
                println!("ue4ss ProcessEvent address not found in log");
            }
        }
        _ => println!(
            "cdo found={} live found={}",
            cdo.is_some(),
            live.len()
        ),
    }
}

#[test]
fn game_thread_ping() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }

    let stats = api.op("pe_stats", json!({}));
    assert!(stats.ok, "pe_stats failed: {:?}", stats.error);
    println!("stats before: {}", stats.result);

    let fires = stats.result["fires"].as_u64().unwrap_or(0);
    if fires == 0 {
        println!("SKIP: PE hook not firing (main menu, or hook not installed yet)");
        return;
    }

    let ping = api.op("pe_ping", json!({}));
    assert!(ping.ok, "pe_ping failed: {:?}", ping.error);
    assert_eq!(
        ping.result["game_thread"],
        json!(true),
        "job did not run: {:?}",
        ping.result
    );

    let after = api.op("pe_stats", json!({}));
    assert!(after.ok);
    println!("stats after: {}", after.result);
    assert!(
        after.result["drained_cmds"].as_u64().unwrap_or(0) >= 1,
        "drain executed no jobs: {:?}",
        after.result
    );
}
