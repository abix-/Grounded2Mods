//! Step 3 of docs/research.md: watch a whole shining
//! cycle.
//!
//! The countdown is proven (section 14). What is not known is
//! what happens at zero: what `TimeUntilEmmision` resets to,
//! whether `EmissionsCount` increments, and whether the reset
//! value matches the configured `ShiningsTimer`. That last one
//! decides whether raising the setting is enough on its own.
//!
//! Ignored by default: it runs for minutes and only makes sense
//! when a shining is actually due.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_cycle -- --ignored --nocapture
//! ```
//!
//! `WWM_WATCH_SECS` (default 420) caps the run.

mod common;
use common::{api_or_skip, offsets_live};
use modforge::client;

const EMISSIONS_COUNT: u64 = 0x2A8;
const TIME_UNTIL_EMMISION: u64 = 0x2B0;
const FREEZE_TIMER: u64 = 0x2B8;

const GLOBAL_MANAGER: &str = "BP_GlobalManager_C";

/// The siren asset is named S_2minSiren, so 120 is where a two
/// minute warning would fire. Logged as it passes so the operator
/// can say whether they heard anything.
const SIREN_MARK: f64 = 120.0;

#[test]
#[ignore = "runs for minutes; only meaningful with a shining due"]
fn watch_cycle() {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let Some(inst) = client::find_live_instance(&api, GLOBAL_MANAGER) else {
        println!("no live {GLOBAL_MANAGER}");
        return;
    };
    let addr = inst.addr;

    let budget: u64 = std::env::var("WWM_WATCH_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(420);

    println!("watching {} for up to {budget}s", inst.addr_selector);
    let start = std::time::Instant::now();
    let mut prev_time: Option<f64> = None;
    let mut prev_count: Option<i32> = None;
    let mut siren_logged = false;

    while start.elapsed().as_secs() < budget {
        let tb = client::read_bytes(&api, addr, TIME_UNTIL_EMMISION, 8);
        let t = if tb.len() >= 8 { Some(client::from_le_f64(&tb, 0)) } else { None };
        let cb = client::read_bytes(&api, addr, EMISSIONS_COUNT, 4);
        let c = if cb.len() >= 4 { Some(client::from_le_i32(&cb, 0)) } else { None };
        let fb = client::read_bytes(&api, addr, FREEZE_TIMER, 1);
        let f = fb.first().copied();
        let secs = start.elapsed().as_secs_f64();

        if let (Some(t), Some(c)) = (t, c) {
            // The reset is the whole point: log it loudly.
            if let Some(pt) = prev_time
                && t > pt + 1.0
            {
                println!(
                    "  *** RESET at t={secs:.1}s: {pt} -> {t}  (EmissionsCount {:?} -> {c})",
                    prev_count
                );
            }
            if prev_count.is_some_and(|pc| pc != c) {
                println!("  *** EmissionsCount {:?} -> {c} at t={secs:.1}s", prev_count);
            }
            if !siren_logged && t <= SIREN_MARK {
                println!("  *** crossed {SIREN_MARK} (siren mark) at t={secs:.1}s");
                siren_logged = true;
            }
            println!(
                "  t={secs:6.1}s  TimeUntilEmmision={t:8.1}  EmissionsCount={c}  Freeze={:?}",
                f
            );
            prev_time = Some(t);
            prev_count = Some(c);
        } else {
            println!("  t={secs:6.1}s  read failed (level change? instance gone?)");
        }

        std::thread::sleep(std::time::Duration::from_secs(5));
    }

    println!("done. final TimeUntilEmmision={prev_time:?} EmissionsCount={prev_count:?}");
}
