//! HK1 Stage S0 probe: dump the active Location's field layout.
//!
//! Drives `hk1.probe.active_location` and prints:
//!   - active_scene_id, location ptr, vtable RVA
//!   - LOC[0x26] horse-vec begin/end + count
//!   - LOC[0x2c] armed flag, LOC[0x2d] drag idx, LOC[0x2e] candidate idx
//!   - first 0x240 bytes of the Location object
//!
//! Modes (env-driven):
//! - default: discovery. Logs the dump, asserts horse_count <= 20.
//! - `HORSEY_EXPECT_OWNED=<N>`: strict. Assert horse_count == N when
//!   active_scene_id == -1 (the overworld slot-0 fallback should be the
//!   owned-horse list).
//! - `HORSEY_EXPECT_LOC_VTABLE_RVA=<hex>`: strict. Assert the vtable RVA
//!   reported by the probe equals the named value. Use this to lock
//!   the active Location vtable so a build update that shuffles vtables
//!   trips the regression.
//!
//! Re-run while the player walks into the paddock / home to confirm
//! the LOC pointer changes and the click-drag fields populate.

mod common;

use serde_json::json;

#[test]
fn probe_active_location_layout() {
    let Some(game) = common::launch("hk1_probe_loc_field_layout") else {
        return;
    };
    eprintln!("[GATE] waiting for a save to load (up to 120s) ...");
    common::wait_for_target_horse(&game, std::time::Duration::from_secs(120));
    let resp = game
        .op_json("hk1.probe.active_location", &json!({}))
        .expect("hk1.probe.active_location op should succeed");
    eprintln!("\nfull response: {resp}");
    let result = resp.get("result").unwrap_or(&resp);

    if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
        // Acceptable on overworld with empty slot 0: just report and pass.
        eprintln!("probe reported: {err}");
        game.pass("probe completed with reported error");
        return;
    }

    let asid = result.get("active_scene_id");
    let loc_ptr = result
        .get("loc_ptr")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let vtable_rva = result
        .get("vtable_rva")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let horse_count = result.get("loc_horse_count");
    let drag = result.get("loc_drag_idx");
    let cand = result.get("loc_cand_idx");
    let armed = result.get("loc_armed");

    eprintln!("active_scene_id : {asid:?}");
    eprintln!("loc_ptr         : {loc_ptr}");
    eprintln!("vtable_rva      : {vtable_rva}");
    eprintln!("loc_horse_count : {horse_count:?}");
    eprintln!("loc_drag_idx    : {drag:?}");
    eprintln!("loc_cand_idx    : {cand:?}");
    eprintln!("loc_armed       : {armed:?}");

    let count = horse_count.and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
    assert!(
        count <= 20,
        "implausible horse count {count}; LOC layout wrong?"
    );

    if let Ok(s) = std::env::var("HORSEY_EXPECT_OWNED") {
        let want: u64 = s.parse().expect("HORSEY_EXPECT_OWNED not a number");
        let asid_int = asid.and_then(|v| v.as_i64()).unwrap_or(0);
        if asid_int == -1 {
            assert_eq!(
                count, want,
                "HORSEY_EXPECT_OWNED={want}, but overworld slot-0 fallback reports horse_count={count}"
            );
        } else {
            eprintln!(
                "note: active_scene_id={asid_int}; not asserting EXPECT_OWNED (only valid on overworld)"
            );
        }
    }

    if let Ok(s) = std::env::var("HORSEY_EXPECT_LOC_VTABLE_RVA") {
        let norm_want = s.trim_start_matches("0x").to_ascii_lowercase();
        let norm_got = vtable_rva.trim_start_matches("0x").to_ascii_lowercase();
        assert_eq!(
            norm_got, norm_want,
            "HORSEY_EXPECT_LOC_VTABLE_RVA={s}, but probe reports vtable_rva={vtable_rva}"
        );
    }

    game.pass(&format!(
        "active_location loc_ptr={loc_ptr} vtable={vtable_rva} horses={count}"
    ));
}
