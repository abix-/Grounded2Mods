//! Hot-path performance counter primitives.
//!
//! Pattern: declare an `AtomicU64` per call site. Bump on every
//! fire. Snapshot the values at T0 and T1; whichever counter has
//! the largest delta is the cycle thief. No locks, single relaxed
//! atomic op per bump, safe to call from any thread.
//!
//! Embedding crates declare their own counter statics (one per
//! call site) and call `bump` / `observe_peak` / `time_scope`
//! against them.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

#[inline(always)]
pub fn bump(c: &AtomicU64) {
    c.fetch_add(1, Ordering::Relaxed);
}

/// Update a high-water-mark counter if `value` exceeds the current.
/// Lock-free via CAS retry. Cheap on the common path (single load
/// + single branch when nothing to do).
#[inline(always)]
pub fn observe_peak(p: &AtomicUsize, value: usize) {
    let mut cur = p.load(Ordering::Relaxed);
    while value > cur {
        match p.compare_exchange_weak(cur, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(c) => cur = c,
        }
    }
}

/// RAII guard that adds the elapsed nanoseconds to `counter` when
/// dropped. Use at the start of any function whose wall time you
/// want to track.
///
/// ```ignore
/// fn hot_path() {
///     let _g = ueforge::counters::time_scope(&MY_TIME_NS);
///     // ... work ...
/// } // _g drops here, counter += elapsed_ns
/// ```
pub struct TimeScope<'a> {
    counter: &'a AtomicU64,
    start: Instant,
}

impl Drop for TimeScope<'_> {
    fn drop(&mut self) {
        let ns = self.start.elapsed().as_nanos() as u64;
        self.counter.fetch_add(ns, Ordering::Relaxed);
    }
}

#[inline(always)]
pub fn time_scope(counter: &AtomicU64) -> TimeScope<'_> {
    TimeScope {
        counter,
        start: Instant::now(),
    }
}

/// Declare one or more `pub static AtomicU64` counter statics, each
/// initialized to zero.
///
/// ```ignore
/// ueforge::counter!(KILL_HOOK_FIRES);
/// ueforge::counter!(KILL_HOOK_PLAYER_FIRES);
/// ueforge::counter!(TIME_NS_DRAIN_PENDING);
///
/// fn on_event() {
///     ueforge::counters::bump(&KILL_HOOK_FIRES);
/// }
/// ```
///
/// The macro is intentionally minimal. Counters are write-mostly,
/// the `AtomicU64` type is the contract, and the snapshot
/// aggregator (which reads them) is game-specific anyway.
#[macro_export]
macro_rules! counter {
    ($name:ident) => {
        pub static $name: ::std::sync::atomic::AtomicU64 =
            ::std::sync::atomic::AtomicU64::new(0);
    };
    ($name:ident, $($rest:ident),+ $(,)?) => {
        $crate::counter!($name);
        $crate::counter!($($rest),+);
    };
}

/// Same as [`counter!`] for `AtomicUsize` peak high-water-mark
/// counters consumed by [`observe_peak`].
#[macro_export]
macro_rules! peak {
    ($name:ident) => {
        pub static $name: ::std::sync::atomic::AtomicUsize =
            ::std::sync::atomic::AtomicUsize::new(0);
    };
    ($name:ident, $($rest:ident),+ $(,)?) => {
        $crate::peak!($name);
        $crate::peak!($($rest),+);
    };
}

/// Build a `serde_json::Value::Object` from a list of
/// `(static_ident => "json_key")` pairs by `load(Relaxed)`-ing each
/// counter (or peak) static. Centralizes the load + ordering
/// discipline so the snapshot endpoint is one short list per mod.
///
/// Accepts any static whose type implements `Load`-shape semantics
/// via `.load(Ordering::Relaxed)` returning a JSON-friendly integer
/// (`AtomicU64`, `AtomicUsize`, `AtomicI64`).
///
/// ```ignore
/// pub fn snapshot_json() -> serde_json::Value {
///     ueforge::counter_json! {
///         KILL_HOOK_FIRES        => "kill_hook_fires",
///         KILL_HOOK_PLAYER_FIRES => "kill_hook_player_fires",
///         DAMAGE_RING_PEAK       => "damage_ring_peak",  // AtomicUsize
///         TIME_NS_DRAIN_PENDING  => "time_ns_drain_pending",
///     }
/// }
/// ```
#[macro_export]
macro_rules! counter_json {
    ( $( $name:path => $key:literal ),* $(,)? ) => {
        ::serde_json::json!({
            $(
                $key: $name.load(::core::sync::atomic::Ordering::Relaxed),
            )*
        })
    };
}

// ---- Timing by name, switched on when wanted ----
//
// The counters above need a static per call site, declared by
// hand. That is fine for a handful of known hot spots and no use
// for the general question "which of the things this mod does is
// slow". For that, work is timed under a NAME, and one report
// lists every name.
//
// Prior art: this is Unreal's own stat system in miniature
// (`SCOPE_CYCLE_COUNTER` plus `stat game`), and Bevy's
// `bevy_diagnostic` does the same with named diagnostics.
//
// OFF by default. Timing costs two clock reads and a lock per
// scope, which is nothing next to reading the object list and is
// not nothing on a path called thousands of times a frame, so it
// is switched on when a measurement is wanted and off the rest of
// the time. Off, a scope costs one atomic load and a branch.

use std::collections::HashMap;
use std::sync::OnceLock;

use parking_lot::Mutex;

/// What one named piece of work has cost since timing was
/// switched on.
#[derive(Default)]
struct Entry {
    calls: AtomicU64,
    time_ns: AtomicU64,
    /// The worst single run. An average hides a stall that
    /// happens once a minute, and a stall once a minute is
    /// exactly what a player notices.
    max_ns: AtomicU64,
}

static TIMING_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Entries are leaked so a scope can hold one without holding the
/// lock. There are as many as there are names, which is tens.
static NAMED: OnceLock<Mutex<HashMap<&'static str, &'static Entry>>> = OnceLock::new();

fn named() -> &'static Mutex<HashMap<&'static str, &'static Entry>> {
    NAMED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Switch timing on or off. Off is the default.
pub fn set_timing(on: bool) {
    TIMING_ON.store(on, Ordering::Relaxed);
}

/// Is timing on?
pub fn timing_on() -> bool {
    TIMING_ON.load(Ordering::Relaxed)
}

/// Time a piece of work under a name.
///
/// Hold the returned value for as long as the work takes; the
/// time is recorded when it drops.
///
/// ```ignore
/// fn watcher() {
///     let _m = counters::measure("misery-spawning");
///     // ... work ...
/// }
/// ```
///
/// Returns a guard that does nothing when timing is off, so the
/// call can be left in place permanently.
#[inline]
pub fn measure(name: &'static str) -> Measure {
    if !timing_on() {
        return Measure { entry: None, start: None };
    }
    let entry = *named().lock().entry(name).or_insert_with(|| {
        Box::leak(Box::new(Entry::default())) as &'static Entry
    });
    Measure { entry: Some(entry), start: Some(Instant::now()) }
}

/// Count one run of a named piece of work without timing it.
///
/// For things that happen far too often to clock individually,
/// like one object being read, where the useful number is how
/// many rather than how long.
#[inline]
pub fn tally(name: &'static str, n: u64) {
    if !timing_on() {
        return;
    }
    let entry = *named().lock().entry(name).or_insert_with(|| {
        Box::leak(Box::new(Entry::default())) as &'static Entry
    });
    entry.calls.fetch_add(n, Ordering::Relaxed);
}

/// Records the time when it drops. See [`measure`].
pub struct Measure {
    entry: Option<&'static Entry>,
    start: Option<Instant>,
}

impl Drop for Measure {
    fn drop(&mut self) {
        let (Some(e), Some(t)) = (self.entry, self.start) else {
            return;
        };
        let ns = t.elapsed().as_nanos() as u64;
        e.calls.fetch_add(1, Ordering::Relaxed);
        e.time_ns.fetch_add(ns, Ordering::Relaxed);
        let mut worst = e.max_ns.load(Ordering::Relaxed);
        while ns > worst {
            match e.max_ns.compare_exchange_weak(
                worst,
                ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(w) => worst = w,
            }
        }
    }
}

/// Every name, slowest first.
///
/// Raw nanoseconds are included as well as milliseconds so two
/// reports taken a second apart can be subtracted: the difference
/// is what that second cost.
pub fn report() -> serde_json::Value {
    let rows = named().lock();
    let mut all: Vec<serde_json::Value> = rows
        .iter()
        .map(|(name, e)| {
            let calls = e.calls.load(Ordering::Relaxed);
            let ns = e.time_ns.load(Ordering::Relaxed);
            let max = e.max_ns.load(Ordering::Relaxed);
            serde_json::json!({
                "name": name,
                "calls": calls,
                "time_ns": ns,
                "total_ms": ns as f64 / 1.0e6,
                "avg_us": if calls > 0 { ns as f64 / calls as f64 / 1.0e3 } else { 0.0 },
                "worst_ms": max as f64 / 1.0e6,
            })
        })
        .collect();
    all.sort_by(|a, b| {
        b["time_ns"].as_u64().unwrap_or(0).cmp(&a["time_ns"].as_u64().unwrap_or(0))
    });
    serde_json::json!({
        "timing_on": timing_on(),
        "measured": all.len(),
        "entries": all,
    })
}

/// The two controls: switch timing on or off, and read what it
/// measured.
///
/// Engine-agnostic, so every forge registers the same pair and a
/// mod for any game is measured the same way.
pub fn register_ops() {
    crate::ops::OP_REGISTRY.register_many([
        crate::ops::OpDef::new(
            "timing",
            "Switch timing on or off, and clear the window. Off by default",
            "{on: bool, reset?: bool}",
            |args| {
                let on = args
                    .get("on")
                    .and_then(serde_json::Value::as_bool)
                    .ok_or("need {on: bool}")?;
                if args
                    .get("reset")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true)
                {
                    reset();
                }
                set_timing(on);
                Ok(serde_json::json!({ "timing_on": timing_on() }))
            },
        ),
        crate::ops::OpDef::new(
            "timing_report",
            "What every named piece of work has cost, slowest first",
            "{}",
            |_args| Ok(report()),
        ),
    ]);
}

/// Forget everything measured so far, so the next window starts
/// from zero.
pub fn reset() {
    for e in named().lock().values() {
        e.calls.store(0, Ordering::Relaxed);
        e.time_ns.store(0, Ordering::Relaxed);
        e.max_ns.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialised, because timing is a global switch.
    static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn off_by_default_records_nothing() {
        let _l = LOCK.lock();
        set_timing(false);
        reset();
        {
            let _m = measure("off-test");
        }
        let r = report();
        let found = r["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == "off-test")
            .map(|e| e["calls"].as_u64().unwrap_or(0))
            .unwrap_or(0);
        assert_eq!(found, 0, "timing is off, so nothing should be recorded");
    }

    #[test]
    fn on_records_calls_and_time() {
        let _l = LOCK.lock();
        set_timing(true);
        reset();
        for _ in 0..3 {
            let _m = measure("on-test");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let r = report();
        let e = r["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == "on-test")
            .cloned()
            .expect("on-test was not recorded");
        set_timing(false);
        assert_eq!(e["calls"], 3);
        assert!(e["total_ms"].as_f64().unwrap() >= 5.0, "got {e}");
        assert!(e["worst_ms"].as_f64().unwrap() >= 1.0, "got {e}");
    }

    /// The slowest thing has to be the first thing read.
    #[test]
    fn the_report_puts_the_slowest_first() {
        let _l = LOCK.lock();
        set_timing(true);
        reset();
        {
            let _m = measure("quick");
        }
        {
            let _m = measure("slow");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let r = report();
        set_timing(false);
        assert_eq!(r["entries"][0]["name"], "slow");
    }

    #[test]
    fn reset_clears_the_window() {
        let _l = LOCK.lock();
        set_timing(true);
        reset();
        {
            let _m = measure("cleared");
        }
        reset();
        let r = report();
        set_timing(false);
        let calls = r["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == "cleared")
            .map(|e| e["calls"].as_u64().unwrap_or(0))
            .unwrap_or(0);
        assert_eq!(calls, 0);
    }
}
