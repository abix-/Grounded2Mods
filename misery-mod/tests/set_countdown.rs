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
use common::{api_or_skip, as_f64, as_i32, first_instance, offsets_live, read_bytes, selector_of};
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

    let Some(inst) = first_instance(&api, GLOBAL_MANAGER) else {
        println!("no live {GLOBAL_MANAGER}");
        return;
    };
    let Some(sel) = selector_of(&inst) else { return };

    let count_before = read_bytes(&api, &sel, EMISSIONS_COUNT, 4).and_then(|b| as_i32(&b));
    let freeze = read_bytes(&api, &sel, FREEZE_TIMER, 1).and_then(|b| b.first().copied());
    let before = read_bytes(&api, &sel, TIME_UNTIL_EMMISION, 8).and_then(|b| as_f64(&b));
    println!("before: TimeUntilEmmision={before:?} EmissionsCount={count_before:?} freeze={freeze:?}");
    if freeze == Some(1) {
        println!("NOTE: FreezeTimer? is set; the countdown will not run. Unfreeze first.");
    }

    let w = api.op(
        "write_bytes",
        json!({"instance_selector": sel, "offset": TIME_UNTIL_EMMISION,
               "bytes_hex": hex::encode(secs.to_le_bytes())}),
    );
    assert!(w.ok, "write_bytes failed: {:?}", w.error);
    println!("wrote TimeUntilEmmision = {secs}");

    // Watch through zero: the reset value and the count increment
    // are the whole reason for forcing this.
    let start = std::time::Instant::now();
    let mut fired = false;
    while start.elapsed().as_secs() < 45 {
        let t = read_bytes(&api, &sel, TIME_UNTIL_EMMISION, 8).and_then(|b| as_f64(&b));
        let c = read_bytes(&api, &sel, EMISSIONS_COUNT, 4).and_then(|b| as_i32(&b));
        println!("  t={:5.1}s  TimeUntilEmmision={t:?}  EmissionsCount={c:?}",
            start.elapsed().as_secs_f64());
        if !fired && c != count_before {
            println!("  *** SHINING: EmissionsCount {count_before:?} -> {c:?}, reset to {t:?}");
            fired = true;
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    println!("fired = {fired}");
}
