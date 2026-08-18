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
use common::{api_or_skip, offsets_live, read_bytes};

#[test]
fn scan_for_settings() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    for class in [
        "BP_GlobalManager_C",
        "BP_SGKGameInstance_C",
    ] {
        let sel = format!("first_class:{class}");
        println!("=== {class} ===");
        scan(&api, &sel);
    }
}

fn scan(api: &common::Api, sel: &str) {
    for off in (0x28..0x1000).step_by(8) {
        let Some(bytes) = read_bytes(&api, sel, off, 8) else {
            continue;
        };
        let val = f64::from_le_bytes(bytes[..8].try_into().unwrap());
        if val > 0.001 && val < 10000.0 && val.is_finite() {
            let marker = if (val - 22.0).abs() < 0.01 { " <-- 22!" } else { "" };
            println!("+0x{off:03x}: {val}{marker}");
        }
    }
}
