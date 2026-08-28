//! Full per-scene data dump for the currently owned horses.
//!
//! Goal: understand WHAT horse data is readable in WHICH scene state. On
//! the bare overworld the home location vector (scene-table slot 0) only
//! holds the pasture horse; the trailer horse is held elsewhere until the
//! home scene is active. This test dumps everything it can in the current
//! scene. Active_scene_id, every owned horse's full field set + genome,
//! and a scan of all 256 scene-table slots so we can see where each horse
//! sits. Then drives into the home scene and dumps again to compare.
//!
//! Run in attach mode against a running game so it reads the live state:
//! set MODFORGE_ATTACH=1 and MODFORGE_SKIP_BUILD=1, then run this single
//! test via cargo-lock with one test thread and nocapture.

mod common;

use serde_json::{Value, json};

#[test]
fn dump_owned_horses() {
    let Some(game) = common::launch("horse_full_dump") else {
        return;
    };

    let dump = |label: &str| {
        eprintln!("\n################ SCENE STATE: {label} ################");
        eprintln!("active_scene_id : {:?}", common::active_scene_id(&game));

        // ---- Owned horses (scene-table slot 0 -> +0x130/+0x138) ----
        let v = game
            .op_json("gamestate.owned_horses", &json!({}))
            .expect("owned_horses op");
        let r = v.get("result").unwrap_or(&v);
        let horses = r
            .get("horses")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        eprintln!("owned count     : {}", horses.len());

        for h in &horses {
            let idx = h.get("idx").and_then(Value::as_u64).unwrap_or(0);
            let ptr = h.get("ptr").and_then(Value::as_str).unwrap_or("0x0");
            eprintln!("\n  -- horse[{idx}] @ {ptr} --");
            eprintln!("    name        : {:?}", h.get("name"));
            eprintln!("    name_id     : {:?}", h.get("name_id"));
            eprintln!("    species     : {:?}", h.get("species"));
            eprintln!(
                "    age/max_age : {:?} / {:?}",
                h.get("age"),
                h.get("max_age")
            );
            eprintln!("    skill       : {:?}", h.get("skill"));
            eprintln!(
                "    tired_a/b   : {:?} / {:?}",
                h.get("tired_a"),
                h.get("tired_b")
            );
            eprintln!(
                "    scene pos   : ({:?}, {:?})",
                h.get("scene_x"),
                h.get("scene_y")
            );
            eprintln!("    container   : {:?}", h.get("container"));

            match game.op_json("horse.read", &json!({ "addr": ptr })) {
                Ok(rv) => {
                    let rr = rv.get("result").unwrap_or(&rv).clone();
                    eprintln!("    litter_stat : {:?}", rr.get("litter_stat"));
                }
                Err(e) => eprintln!("    horse.read err: {e}"),
            }

            match game.op_json("horse.vanilla.genome.get", &json!({ "addr": ptr })) {
                Ok(gv) => {
                    let gg = gv.get("result").unwrap_or(&gv).clone();
                    if let Some(alleles) = gg.get("alleles").and_then(Value::as_array) {
                        let vals: Vec<u64> = alleles.iter().filter_map(Value::as_u64).collect();
                        let nonzero = vals.iter().filter(|&&x| x != 0).count();
                        let max = vals.iter().copied().max().unwrap_or(0);
                        eprintln!(
                            "    genome      : {} bytes, {} non-zero, max tier {}",
                            vals.len(),
                            nonzero,
                            max
                        );
                        eprintln!("    genome raw  : {vals:?}");
                    } else {
                        eprintln!("    genome      : {gg:?}");
                    }
                }
                Err(e) => eprintln!("    genome err  : {e}"),
            }
        }

        // ---- Where ARE the horses: scan every scene-table slot ----
        eprintln!("\n  -- scene-table slot scan (GS+0x438) --");
        match game.op_json("gamestate.scan_438_slots", &json!({})) {
            Ok(sv) => {
                let ss = sv.get("result").unwrap_or(&sv).clone();
                eprintln!(
                    "{}",
                    serde_json::to_string_pretty(&ss).unwrap_or_else(|_| ss.to_string())
                );
            }
            Err(e) => eprintln!("    scan_438_slots err: {e}"),
        }
    };

    // Phase 1: whatever scene we attached to (expected: overworld).
    dump("PHASE 1 (current scene on attach)");

    // Phase 2: drive into the home scene, where both pasture + trailer
    // horses load into slot 0. Non-fatal: if the synthetic house-door
    // click doesn't reach the game window, we still report phase 1 and the
    // failed-entry note, then the operator can enter the scene by hand.
    eprintln!("\n>>> attempting to enter the home scene via synthetic click...");
    let entered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        common::ensure_home_scene_loaded(&game, std::time::Duration::from_secs(15));
    }))
    .is_ok();
    eprintln!(
        ">>> home-scene entry {}",
        if entered {
            "OK"
        } else {
            "FAILED (see note above)"
        }
    );

    dump("PHASE 2 (after home-scene entry attempt)");

    // Decode specific name_ids directly via the name resolver, so we can
    // read the name of a horse that is NOT currently in the owned list
    // (e.g. the trailer horse, held off-list on the overworld). SSO names
    // (size <= 15) are inline in the first 0x40 bytes name_diag returns.
    eprintln!("\n################ NAME DECODE (direct by name_id) ################");
    for nid in [344u64, 345u64] {
        match game.op_json("horse.name_diag", &json!({ "name_id": nid })) {
            Ok(dv) => {
                let dd = dv.get("result").unwrap_or(&dv).clone();
                let size = dd.get("size_at_18").and_then(Value::as_u64).unwrap_or(0) as usize;
                let bytes: Vec<u8> = dd
                    .get("bytes_00_3f")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .split_whitespace()
                    .filter_map(|h| u8::from_str_radix(h, 16).ok())
                    .collect();
                let name = if size > 0 && size <= 15 && size <= bytes.len() {
                    String::from_utf8_lossy(&bytes[..size]).into_owned()
                } else {
                    format!("(size {size}; heap or unreadable, follow first_qword)")
                };
                eprintln!("name_id {nid} -> {name:?}  (size {size})");
            }
            Err(e) => eprintln!("name_id {nid} name_diag err: {e}"),
        }
    }
}
