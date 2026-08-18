//! Freeze the shining countdown.
//!
//! `BP_GlobalManager_C` has a `FreezeTimer?` bool at 0x2B8 (the
//! question mark is part of the name) and `FreezeTime` /
//! `UnfreezeTime` functions. The control plane has no function
//! call op, so this writes the bool directly and then proves it
//! worked by sampling the countdown: frozen means the value
//! stops moving.
//!
//! Ignored by default because it changes live game state.
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test freeze_timer freeze -- --ignored --nocapture
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test freeze_timer unfreeze -- --ignored --nocapture
//! ```

mod common;
use common::{api_or_skip, as_f64, first_instance, offsets_live, read_bytes, selector_of};
use serde_json::json;

const TIME_UNTIL_EMMISION: u64 = 0x2B0;
const FREEZE_TIMER: u64 = 0x2B8;
const GLOBAL_MANAGER: &str = "BP_GlobalManager_C";

#[test]
#[ignore = "changes live game state on purpose"]
fn freeze() {
    set_freeze(true);
}

#[test]
#[ignore = "changes live game state on purpose"]
fn unfreeze() {
    set_freeze(false);
}

fn set_freeze(on: bool) {
    let Some(api) = api_or_skip() else { return };
    if !offsets_live(&api) {
        println!("SKIP: offsets not live");
        return;
    }
    let Some(inst) = first_instance(&api, GLOBAL_MANAGER) else {
        println!("no live {GLOBAL_MANAGER}");
        return;
    };
    let Some(sel) = selector_of(&inst) else { return };

    let before = read_bytes(&api, &sel, TIME_UNTIL_EMMISION, 8).and_then(|b| as_f64(&b));
    println!("before: TimeUntilEmmision={before:?} freeze={:?}",
        read_bytes(&api, &sel, FREEZE_TIMER, 1).and_then(|b| b.first().copied()));

    let w = api.op(
        "write_bytes",
        json!({"instance_selector": sel, "offset": FREEZE_TIMER,
               "bytes_hex": if on { "01" } else { "00" }}),
    );
    assert!(w.ok, "write_bytes failed: {:?}", w.error);
    println!("wrote FreezeTimer? = {}", on as u8);

    // The write landing is not the point; the countdown stopping
    // is. Sample either side of a wait and compare.
    let t0 = read_bytes(&api, &sel, TIME_UNTIL_EMMISION, 8).and_then(|b| as_f64(&b));
    std::thread::sleep(std::time::Duration::from_secs(6));
    let t1 = read_bytes(&api, &sel, TIME_UNTIL_EMMISION, 8).and_then(|b| as_f64(&b));
    let flag = read_bytes(&api, &sel, FREEZE_TIMER, 1).and_then(|b| b.first().copied());

    println!("after 6s: {t0:?} -> {t1:?}  freeze flag={flag:?}");
    match (t0, t1) {
        (Some(a), Some(b)) if on => {
            let moved = a - b;
            println!("countdown moved {moved} in 6s (frozen means ~0)");
            assert!(
                moved.abs() < 1.0,
                "FreezeTimer? was set but the countdown still ran ({a} -> {b})"
            );
        }
        (Some(a), Some(b)) => {
            println!("countdown moved {} in 6s (unfrozen means ~6)", a - b);
        }
        _ => println!("could not read the countdown"),
    }
}
