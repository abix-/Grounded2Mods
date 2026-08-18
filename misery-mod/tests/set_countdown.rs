//! Set the shining countdown to a chosen number of seconds, then
//! watch what happens.
//!
//! Writing `TimeUntilEmmision` (BP_GlobalManager_C + 0x2B0) to a
//! small value forces a shining within seconds instead of waiting
//! out a 22 minute interval. That is the only practical way to
//! observe the event repeatedly.
//!
//! This changes live game state and can get the player killed:
//! `RespawnOnEmission` is on for this save.
//!
//! ```text
//! MISERY_SECS=10 MISERY_DEBUG_PORT=17176 cargo test -p misery-mod \
//!   --test set_countdown -- --ignored --nocapture
//! ```

mod common;
use common::{api_or_skip, offsets_live};
use modforge::client::research;
use serde_json::json;

const EMISSIONS_COUNT: u64 = 0x2A8;
const TIME_UNTIL_EMMISION: u64 = 0x2B0;
const FREEZE_TIMER: u64 = 0x2B8;
const GLOBAL_MANAGER: &str = "BP_GlobalManager_C";

#[test]
#[ignore = "forces a shining on the live save"]
fn set_countdown() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let secs: f64 = std::env::var("MISERY_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10.0);

    let Some(inst) = research::find_live_instance(&api, GLOBAL_MANAGER) else {
        println!("no live {GLOBAL_MANAGER}");
        return;
    };
    let addr = inst.addr;
    let sel = &inst.addr_selector;

    let count_before = research::read_i32(&api, addr, EMISSIONS_COUNT);
    let freeze = research::read_u8(&api, addr, FREEZE_TIMER);
    let before = research::read_f64(&api, addr, TIME_UNTIL_EMMISION);
    println!("before: TimeUntilEmmision={before} EmissionsCount={count_before} freeze={freeze}");
    if freeze == 1 {
        println!("NOTE: FreezeTimer? is set; the countdown will not run. Unfreeze first.");
    }

    let w = api.op(
        "write_bytes",
        json!({"instance_selector": sel, "offset": TIME_UNTIL_EMMISION,
               "bytes_hex": hex::encode(secs.to_le_bytes())}),
    );
    assert!(w.ok, "write_bytes failed: {:?}", w.error);
    println!("wrote TimeUntilEmmision = {secs}");

    let start = std::time::Instant::now();
    let mut fired = false;
    while start.elapsed().as_secs() < 45 {
        let t = research::read_f64(&api, addr, TIME_UNTIL_EMMISION);
        let c = research::read_i32(&api, addr, EMISSIONS_COUNT);
        println!("  t={:5.1}s  TimeUntilEmmision={t}  EmissionsCount={c}",
            start.elapsed().as_secs_f64());
        if !fired && c != count_before {
            println!("  *** SHINING: EmissionsCount {count_before} -> {c}, reset to {t}");
            fired = true;
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    println!("fired = {fired}");
}
