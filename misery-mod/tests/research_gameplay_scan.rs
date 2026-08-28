//! Scan BP_GlobalManager_C for the GameplaySettings struct.
//!
//! ShiningsTimer should be 22.0 (the configured interval).
//! Read every 8-byte aligned f64 from +0x200 to +0x800 and
//! print values that look like real game settings (not zero,
//! not garbage).
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_gameplay_scan -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use modforge::client;

#[test]
fn scan_for_settings() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    for class in ["BP_GlobalManager_C", "BP_SGKGameInstance_C"] {
        println!("=== {class} ===");
        let Some(inst) = client::find_live_instance(&api, class) else {
            println!("  no live instance");
            continue;
        };
        scan(&api, inst.addr);
    }
}

fn scan(api: &common::Api, addr: u64) {
    for off in (0x28..0x1000).step_by(8) {
        let bytes = client::read_bytes(api, addr, off, 8);
        if bytes.len() < 8 {
            continue;
        }
        let val = client::from_le_f64(&bytes, 0);
        if val > 0.001 && val < 10000.0 && val.is_finite() {
            let marker = if (val - 22.0).abs() < 0.01 {
                " <-- 22!"
            } else {
                ""
            };
            println!("+0x{off:03x}: {val}{marker}");
        }
    }
}
