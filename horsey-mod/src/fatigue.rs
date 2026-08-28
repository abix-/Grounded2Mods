//! Fatigue suppressor: per-frame loop that zeros only the
//! race-eligibility tiredness flag (`horse + 0x205`) on every
//! live horse, while leaving the sleep-gate flag (`horse + 0x206`)
//! alone.
//!
//! Why we don't use the game's built-in `no_tire` cheat
//! (`DAT_1403d95c5`): that cheat's per-frame loop at
//! `0x1400ce6c0`-area (line 121172 of decompiled/all_functions.c)
//! zeroes BOTH +0x205 AND +0x206. +0x206 is what the sleep
//! handler at `0x1400e0aa0` (line 131551-131573 of all_functions.c)
//! checks for "is any horse tired enough to need sleep". With +0x206
//! always zero, the player gets "None of your horses are tired"
//! and can't advance the day cycle.
//!
//! Splitting the two flags lets us:
//! - Race any horse, any time (race gate reads +0x205; we zero it).
//! - Sleep to advance day (sleep gate reads +0x206; we leave it).
//!
//! Implementation: a worker thread on a 50ms timer that walks the
//! horse list at GameState `+0x130/+0x138` and clears `+0x205`.
//! The game's own loop runs once per frame (~16ms at 60fps); ours
//! runs slightly slower but still fast enough that no eligibility
//! check sees a stale value.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::gamestate;
use crate::targets::horse_offset;

/// Suppressor on/off. Polled by the worker thread each iteration.
static ENABLED: AtomicBool = AtomicBool::new(true);

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
    modforge::log!("horsey-mod: fatigue suppressor -> {on}");
}

/// Spawn the worker. Idempotent: only the first call starts a
/// thread; subsequent calls are no-ops.
pub fn spawn_suppressor() {
    static SPAWNED: AtomicBool = AtomicBool::new(false);
    if SPAWNED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("horsey-fatigue".into())
        .spawn(run);
}

fn run() {
    let interval = Duration::from_millis(50);
    loop {
        std::thread::sleep(interval);
        if !ENABLED.load(Ordering::Relaxed) {
            continue;
        }
        clear_race_flag_on_all_live_horses();
    }
}

/// Walk gamestate + 0x130/+0x138 and zero the race-eligibility
/// flag on every live horse. Leaves +0x206 untouched.
fn clear_race_flag_on_all_live_horses() {
    let n = gamestate::live_horse_count();
    for i in 0..n {
        let Some(h) = gamestate::live_horse_ptr(i) else {
            continue;
        };
        if h == 0 {
            continue;
        }
        // SAFETY: live_horse_ptr returned a non-null Horse* from
        // the engine's vector. The +0x205 byte is at a stable
        // offset for every Horse object. Writing 0 is the same
        // operation the game's own per-frame loop does to all
        // horses when no_tire is on.
        unsafe {
            *((h + horse_offset::TIRED_FLAG_A) as *mut u8) = 0;
        }
    }
}
