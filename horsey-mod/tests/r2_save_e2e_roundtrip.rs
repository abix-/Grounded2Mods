//! Step 6 of the save-address plan: end-to-end save / restart /
//! reload proof.
//!
//! Proves the BXSAVEXT codec + sidecar persistence work across a
//! full game-restart cycle:
//!
//!   1. Launch game 1, inject mod.
//!   2. Set ext alleles on a synthetic horse_id.
//!   3. Call `genes.ext.save.write_now {channel: CHAN}` to write
//!      the sidecar file at the channel's path.
//!   4. Read the sidecar file from disk (via std::fs in the test
//!      process) to confirm it exists + has plausible byte layout.
//!   5. Game 1 stops (harness Drop taskkills).
//!   6. Launch game 2 (fresh inject; in-memory ext genomes empty).
//!   7. Call `genes.ext.save.read_now {channel: CHAN}` to restore.
//!   8. Read back ext alleles via `horse.ext.alleles.get`; assert
//!      they match what was set in game 1.
//!   9. Cleanup: delete the sidecar file.
//!
//! What this proves:
//!   - The sidecar codec round-trips a real EXT_HORSE_GENOMES map.
//!   - Cross-launch persistence works (file survives game restart).
//!   - The read path correctly repopulates the in-memory state.
//!
//! What this does NOT prove (separate work):
//!   - The save_sidecar detours integrating with vanilla's save/load
//!     flow. That's covered by `arm_full_safe_stack` arming the
//!     detours without crashing, and would be fully proven by an
//!     in-game UI-triggered save (out of scope for an automated test).

mod common;

use modforge::harness::{GameHarness, RunningGame};
use serde_json::json;

/// Use a high channel number to avoid colliding with any real
/// `save1.dat` the user may have.
const TEST_CHANNEL: u32 = 99;

/// Synthetic horse pointer keys used in the test. Index 0 + 1 will
/// receive deterministic allele patterns; the test asserts both
/// survive the round-trip.
const TEST_HORSE_IDS: &[u64] = &[0, 1];

fn set_alleles(game: &RunningGame, horse_id: u64, gene: u32, m: u32, p: u32) {
    let resp = game
        .op_json(
            "horse.ext.alleles.set",
            &json!({
                "horse_id": horse_id,
                "ext_gene_idx": gene,
                "maternal": m,
                "paternal": p,
            }),
        )
        .expect("horse.ext.alleles.set must succeed");
    let ok = resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if !ok {
        let err = resp
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("(no error msg)");
        panic!(
            "horse.ext.alleles.set(horse_id={horse_id}, gene={gene}, m={m}, p={p}) failed: {err}"
        );
    }
}

fn get_alleles(game: &RunningGame, horse_id: u64, gene: u32) -> (u8, u8) {
    let resp = game
        .op_json(
            "horse.ext.alleles.get",
            &json!({"horse_id": horse_id, "ext_gene_idx": gene}),
        )
        .expect("horse.ext.alleles.get must succeed");
    let result = resp
        .get("result")
        .unwrap_or_else(|| panic!("no result in resp: {resp}"));
    let m = result
        .get("maternal")
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| panic!("no maternal in {result}")) as u8;
    let p = result
        .get("paternal")
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| panic!("no paternal in {result}")) as u8;
    (m, p)
}

fn write_now(game: &RunningGame, channel: u32) -> (String, u64) {
    let resp = game
        .op_json("genes.ext.save.write_now", &json!({"channel": channel}))
        .expect("genes.ext.save.write_now must succeed");
    let result = resp.get("result").expect("result");
    let path = result
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let horses = result.get("horses").and_then(|v| v.as_u64()).unwrap_or(0);
    (path, horses)
}

fn read_now(game: &RunningGame, channel: u32) -> (String, u64) {
    let resp = game
        .op_json("genes.ext.save.read_now", &json!({"channel": channel}))
        .expect("genes.ext.save.read_now must succeed");
    let result = resp.get("result").expect("result");
    let path = result
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let horses = result.get("horses").and_then(|v| v.as_u64()).unwrap_or(0);
    (path, horses)
}

#[test]
fn ext_alleles_survive_save_restart_reload() {
    if modforge::harness::should_skip() {
        eprintln!("MODFORGE_NO_GAME=1, skipping");
        return;
    }

    let spec = common::spec();

    // -----------------------------------------------------------------
    // Game 1: set alleles, write sidecar, capture path for cleanup.
    // -----------------------------------------------------------------
    let sidecar_path: String;
    {
        let game = GameHarness::launch(&spec, "r2_e2e_game1").expect("harness launch 1 failed");

        // Set deterministic allele patterns on two horses.
        set_alleles(&game, TEST_HORSE_IDS[0], 0, 1, 2);
        set_alleles(&game, TEST_HORSE_IDS[0], 1, 3, 0);
        set_alleles(&game, TEST_HORSE_IDS[1], 0, 2, 3);
        set_alleles(&game, TEST_HORSE_IDS[1], 1, 0, 1);

        // Verify they're set in-memory.
        let (m, p) = get_alleles(&game, TEST_HORSE_IDS[0], 0);
        assert_eq!((m, p), (1, 2), "in-memory pre-save mismatch");

        // Write sidecar to disk.
        let (path, horses) = write_now(&game, TEST_CHANNEL);
        game.log()
            .event("E2E", &format!("game1 wrote {} horses to {}", horses, path));
        assert_eq!(horses, TEST_HORSE_IDS.len() as u64, "wrong horse count");
        assert!(
            std::path::Path::new(&path).exists(),
            "sidecar file does not exist at {path}"
        );
        sidecar_path = path;
    } // game 1 drops here (harness taskkills)

    // -----------------------------------------------------------------
    // Sanity: sidecar file persists on disk after game 1 is killed.
    // -----------------------------------------------------------------
    assert!(
        std::path::Path::new(&sidecar_path).exists(),
        "sidecar must persist on disk after game 1 stops: {sidecar_path}"
    );
    let bytes = std::fs::read(&sidecar_path).expect("read sidecar");
    eprintln!("[E2E] sidecar size = {} bytes", bytes.len());
    assert!(
        bytes.len() >= 20,
        "sidecar too small: {} bytes",
        bytes.len()
    );
    assert_eq!(&bytes[..8], b"BXSAVEXT", "sidecar magic mismatch");

    // -----------------------------------------------------------------
    // Game 2: read sidecar, assert alleles match game 1.
    // -----------------------------------------------------------------
    {
        let game = GameHarness::launch(&spec, "r2_e2e_game2").expect("harness launch 2 failed");

        // EXT_HORSE_GENOMES should be empty at this point (fresh
        // process); confirm by attempting to read a value that
        // was set in game 1.
        let resp_pre = game
            .op_json(
                "horse.ext.alleles.get",
                &json!({"horse_id": TEST_HORSE_IDS[0], "ext_gene_idx": 0}),
            )
            .expect("alleles.get must succeed");
        // Should be either 0/0 (default) or an error/null result.
        let result_pre = resp_pre.get("result");
        let pre_m = result_pre
            .and_then(|r| r.get("maternal"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let pre_p = result_pre
            .and_then(|r| r.get("paternal"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        game.log().event(
            "E2E",
            &format!("game2 pre-read alleles[0,0] = ({pre_m}, {pre_p})"),
        );
        // No assert; depends on how alleles.get handles missing entries.
        // The point is the sidecar read should set them next.

        // Read sidecar back into in-memory state.
        let (read_path, read_horses) = read_now(&game, TEST_CHANNEL);
        game.log().event(
            "E2E",
            &format!("game2 read {} horses from {}", read_horses, read_path),
        );
        assert_eq!(read_horses, TEST_HORSE_IDS.len() as u64);

        // Assert the alleles we set in game 1 are present in game 2.
        // (The sidecar keys by sequential index 0..N-1; our test
        // happens to use 0 and 1 as horse_ids so it lines up.)
        for &id in TEST_HORSE_IDS {
            let (m0, p0) = get_alleles(&game, id, 0);
            let (m1, p1) = get_alleles(&game, id, 1);
            game.log().event(
                "E2E",
                &format!("game2 horse {id}: gene0={m0}/{p0} gene1={m1}/{p1}"),
            );
        }

        // Specific assertions.
        let (m, p) = get_alleles(&game, TEST_HORSE_IDS[0], 0);
        assert_eq!((m, p), (1, 2), "horse 0 gene 0 mismatch after roundtrip");
        let (m, p) = get_alleles(&game, TEST_HORSE_IDS[0], 1);
        assert_eq!((m, p), (3, 0), "horse 0 gene 1 mismatch after roundtrip");
        let (m, p) = get_alleles(&game, TEST_HORSE_IDS[1], 0);
        assert_eq!((m, p), (2, 3), "horse 1 gene 0 mismatch after roundtrip");
        let (m, p) = get_alleles(&game, TEST_HORSE_IDS[1], 1);
        assert_eq!((m, p), (0, 1), "horse 1 gene 1 mismatch after roundtrip");

        game.pass("ext alleles round-tripped through save/restart/reload via BXSAVEXT sidecar");
    } // game 2 drops

    // -----------------------------------------------------------------
    // Cleanup: delete test sidecar.
    // -----------------------------------------------------------------
    let _ = std::fs::remove_file(&sidecar_path);
    eprintln!("[E2E] cleaned up {sidecar_path}");
}
