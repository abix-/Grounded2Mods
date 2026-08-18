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
use common::{api_or_skip, offsets_live};
use modforge::client::research;
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
    let Some(inst) = research::find_live_instance(&api, GLOBAL_MANAGER) else {
        println!("no live {GLOBAL_MANAGER}");
        return;
    };
    let addr = inst.addr;
    let sel = &inst.addr_selector;

    let before = research::read_f64(&api, addr, TIME_UNTIL_EMMISION);
    let freeze_flag = research::read_u8(&api, addr, FREEZE_TIMER);
    println!("before: TimeUntilEmmision={before} freeze={freeze_flag}");

    let w = api.op(
        "write_bytes",
        json!({"instance_selector": sel, "offset": FREEZE_TIMER,
               "bytes_hex": if on { "01" } else { "00" }}),
    );
    assert!(w.ok, "write_bytes failed: {:?}", w.error);
    println!("wrote FreezeTimer? = {}", on as u8);

    let t0 = research::read_f64(&api, addr, TIME_UNTIL_EMMISION);
    std::thread::sleep(std::time::Duration::from_secs(6));
    let t1 = research::read_f64(&api, addr, TIME_UNTIL_EMMISION);
    let flag = research::read_u8(&api, addr, FREEZE_TIMER);

    println!("after 6s: {t0} -> {t1}  freeze flag={flag}");
    if on {
        let moved = t0 - t1;
        println!("countdown moved {moved} in 6s (frozen means ~0)");
        assert!(
            moved.abs() < 1.0,
            "FreezeTimer? was set but the countdown still ran ({t0} -> {t1})"
        );
    } else {
        println!("countdown moved {} in 6s (unfrozen means ~6)", t0 - t1);
    }
}
