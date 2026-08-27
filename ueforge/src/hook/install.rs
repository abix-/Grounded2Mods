//! Hook-install retry/backoff helper.
//!
//! UE classes show up in GObjects on the engine's schedule, not
//! ours. `on_unreal_init` fires before most blueprint-generated
//! classes load (the inventory widget class isn't there until the
//! player opens the inventory the first time). Every UE4SS-Rust mod
//! ends up with the same retry-with-exponential-backoff loop around
//! its hook installs.
//!
//! ```ignore
//! match ueforge::hook::install_with_backoff(
//!     "inv hook",
//!     ueforge::hook::RetryPolicy::default_install(),
//!     || inv_hook::install(slot_count),
//! ) {
//!     Some(h) => { ueforge::log!("inv hook: installed"); std::mem::forget(h); }
//!     None    => { ueforge::log!("inv hook: gave up"); }
//! }
//! ```
//!
//! Logs "pending" lines as the install attempt errors change so the
//! mod log shows the engine-load progression. Logs once on timeout.

use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::ue::{UFunction, UObject};

use super::OriginalProcessEvent;

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub base: Duration,
    pub max: Duration,
    pub timeout: Duration,
}

impl RetryPolicy {
    pub const fn new(base: Duration, max: Duration, timeout: Duration) -> Self {
        Self { base, max, timeout }
    }

    /// 500ms base, 5s cap, 10min timeout. Matches g2rpg's tuning,
    /// which has held across both inventory and damage hooks.
    pub const fn default_install() -> Self {
        Self::new(
            Duration::from_millis(500),
            Duration::from_secs(5),
            Duration::from_secs(600),
        )
    }
}

/// Repeatedly call `try_install` with exponential backoff (capped
/// by `policy.max`) until it returns `Ok` or `policy.timeout`
/// elapses. Returns `Some(h)` on success, `None` after timeout.
///
/// `name` is used for log lines only.
pub fn install_with_backoff<H, F>(name: &str, policy: RetryPolicy, mut try_install: F) -> Option<H>
where
    F: FnMut() -> Result<H, &'static str>,
{
    let mut delay = policy.base;
    let deadline = Instant::now() + policy.timeout;
    let mut last_err: Option<&str> = None;
    loop {
        match try_install() {
            Ok(h) => return Some(h),
            Err(e) => {
                if last_err != Some(e) {
                    crate::log!("{name}: pending ({e}), will retry");
                    last_err = Some(e);
                }
            }
        }
        if Instant::now() >= deadline {
            crate::log!("{name}: gave up after {:?}", policy.timeout);
            return None;
        }
        std::thread::sleep(jitter(delay));
        delay = (delay * 2).min(policy.max);
    }
}

/// Pseudo-random +/-25% jitter on `delay`. Multiple mods using
/// the same `RetryPolicy::default_install()` would otherwise wake
/// on the same beat and hammer the same load events (e.g. an
/// engine GC pass right after a level load). Spreading the
/// retries decorrelates them.
fn jitter(delay: Duration) -> Duration {
    // Map fastrand into [-25%, +25%] of delay.
    let frac = fastrand::i64(-250..=250);
    let base = delay.as_nanos() as i64;
    let bumped = (base + base * frac / 1000).max(0) as u64;
    Duration::from_nanos(bumped)
}

/// Install a hook **once** (no retry), log the outcome, and
/// register the handle into the framework's hot-reload teardown
/// registry on success.
///
/// The universal "install, log, register" pattern every mod's
/// `worker()` runs once per hook. Replaces the hand-rolled
/// `match try_install() { Ok(h) => { log!; forget(h); } Err(e)
/// => log!; }` triplet at every call site.
///
/// `label` is used for log lines (e.g. `"rpg/kill"`). The handle
/// is moved into [`crate::hook::register`] so the hook stays
/// installed until either process teardown or
/// [`crate::hook::shutdown_all`] tears it down (the framework
/// calls `shutdown_all` from `ueforge_mod_shutdown`).
/// `class_name_fn` extracts the hook target's class for the
/// success log line.
pub fn install_immediate_or_log<F>(
    label: &str,
    try_install: F,
    class_name_fn: impl FnOnce(&crate::hook::ProcessEventHook) -> &str,
) -> bool
where
    F: FnOnce() -> Result<crate::hook::ProcessEventHook, &'static str>,
{
    match try_install() {
        Ok(h) => {
            crate::log!("{label}: installed on {}", class_name_fn(&h));
            crate::hook::register(h);
            true
        }
        Err(e) => {
            crate::log!("{label}: install failed ({e})");
            false
        }
    }
}

/// Poll for a live UObject on the game thread, install its
/// ProcessEvent hook once, and register the handle for framework
/// shutdown. The finder and hook installation never run on the
/// poller's background thread.
///
/// `report` receives installation failures and one success so the
/// game mod can retain its own feature-specific log wording.
///
/// See [`install_for_live_object_until`] for a hook that should be
/// taken back out once its job is done.
pub fn install_for_live_object<F, H, R>(
    poller_name: &'static str,
    poll_interval: Duration,
    class_name: &'static str,
    find_object: F,
    handler: H,
    report: R,
) where
    F: Fn() -> Option<&'static UObject> + Send + Sync + 'static,
    H: Fn(&UObject, &UFunction, *mut c_void, OriginalProcessEvent) + Send + Sync + Clone + 'static,
    R: Fn(Result<(), &'static str>) + Send + Sync + 'static,
{
    install_for_live_object_inner(
        poller_name,
        poll_interval,
        class_name,
        find_object,
        handler,
        report,
        // Never finished: the hook stays for the session and the
        // poller stops as soon as it is installed.
        || false,
        false,
    );
}

/// The same, for a hook that is finished once something has
/// happened.
///
/// `done` is asked, off the game thread, once the hook is in. The
/// first time it answers true the hook is uninstalled and the
/// poller ends itself, so neither costs anything for the rest of
/// the session.
///
/// Two things this fixes, both measured in MISERY on 2026-08-26:
///
///   - The poller used to run for the life of the process, hopping
///     to the game thread every tick to discover it had nothing to
///     do. The notice watcher spent 1326 ms in 30 seconds doing
///     exactly that, most of it queued behind real work.
///   - The hook used to stay installed forever. A Blueprint widget
///     class shares the base widget vtable, so a widget hook fires
///     for EVERY widget in the game, long after the one it was
///     installed for is gone.
///
/// The finished check runs on the poller's own thread, never
/// inside the handler, because uninstalling waits for calls
/// already inside the hook to leave and the handler would be one
/// of them.
pub fn install_for_live_object_until<F, H, R, D>(
    poller_name: &'static str,
    poll_interval: Duration,
    class_name: &'static str,
    find_object: F,
    handler: H,
    report: R,
    done: D,
) where
    F: Fn() -> Option<&'static UObject> + Send + Sync + 'static,
    H: Fn(&UObject, &UFunction, *mut c_void, OriginalProcessEvent) + Send + Sync + Clone + 'static,
    R: Fn(Result<(), &'static str>) + Send + Sync + 'static,
    D: Fn() -> bool + Send + Sync + 'static,
{
    install_for_live_object_inner(
        poller_name,
        poll_interval,
        class_name,
        find_object,
        handler,
        report,
        done,
        true,
    );
}

fn install_for_live_object_inner<F, H, R, D>(
    poller_name: &'static str,
    poll_interval: Duration,
    class_name: &'static str,
    find_object: F,
    handler: H,
    report: R,
    done: D,
    remove_when_done: bool,
) where
    F: Fn() -> Option<&'static UObject> + Send + Sync + 'static,
    H: Fn(&UObject, &UFunction, *mut c_void, OriginalProcessEvent) + Send + Sync + Clone + 'static,
    R: Fn(Result<(), &'static str>) + Send + Sync + 'static,
    D: Fn() -> bool + Send + Sync + 'static,
{
    let installed = Arc::new(AtomicBool::new(false));
    let tick_installed = installed.clone();
    let install_on_game_thread = crate::game_thread::each_tick(move || {
        let Some(object) = find_object() else {
            return;
        };
        match crate::hook::ProcessEventHook::install_for_object(class_name, object, handler.clone())
        {
            Ok(hook) => {
                crate::hook::register(hook);
                tick_installed.store(true, Ordering::Release);
                report(Ok(()));
            }
            Err(error) => report(Err(error)),
        }
    });

    let handle: Arc<std::sync::OnceLock<modforge::rpg::poller::PollerHandle>> =
        Arc::new(std::sync::OnceLock::new());
    let tick_handle = handle.clone();
    let tick = move || {
        // Both checks are on the poller's own thread. Hopping to
        // the game thread to find out there is nothing to do is
        // what made this expensive.
        if !installed.load(Ordering::Acquire) {
            install_on_game_thread();
            return;
        }
        if remove_when_done && !done() {
            return;
        }
        if remove_when_done {
            crate::hook::remove(class_name);
        }
        // A session hook needs no more install checks. A temporary
        // hook is now finished and has been removed.
        if let Some(h) = tick_handle.get() {
            h.stop_soon();
        }
    };

    let spawned = modforge::rpg::poller::spawn_interval(poller_name, poll_interval, tick);
    // The tick reads this to end itself. Setting it after the
    // spawn is safe: a tick that runs first simply finds nothing
    // and ends on the following one.
    let _ = handle.set(spawned);
    // Deliberate leak, and it MUST stay. Dropping the last
    // reference runs `PollerHandle::drop`, which joins the worker
    // thread. The last reference is the tick closure, which the
    // worker drops as it exits, so the thread would be joining
    // itself. Holding one reference forever means that drop never
    // runs.
    std::mem::forget(handle);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn succeeds_on_first_try() {
        let policy = RetryPolicy::new(
            Duration::from_millis(1),
            Duration::from_millis(10),
            Duration::from_millis(100),
        );
        let result: Option<&'static str> = install_with_backoff("test", policy, || Ok("hook"));
        assert_eq!(result, Some("hook"));
    }

    #[test]
    fn retries_then_succeeds() {
        let attempts = AtomicU32::new(0);
        let policy = RetryPolicy::new(
            Duration::from_millis(1),
            Duration::from_millis(10),
            Duration::from_millis(500),
        );
        let result: Option<u32> = install_with_backoff("test", policy, || {
            let n = attempts.fetch_add(1, Ordering::Relaxed);
            if n < 3 { Err("not ready") } else { Ok(n) }
        });
        assert_eq!(result, Some(3));
        assert_eq!(attempts.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn times_out() {
        let policy = RetryPolicy::new(
            Duration::from_millis(1),
            Duration::from_millis(2),
            Duration::from_millis(20),
        );
        let result: Option<()> = install_with_backoff("test", policy, || Err("never"));
        assert_eq!(result, None);
    }

    #[test]
    fn default_policy_values() {
        let p = RetryPolicy::default_install();
        assert_eq!(p.base, Duration::from_millis(500));
        assert_eq!(p.max, Duration::from_secs(5));
        assert_eq!(p.timeout, Duration::from_secs(600));
    }

    #[test]
    fn jitter_stays_within_25_percent() {
        let base = Duration::from_millis(1000);
        // Hammer the jitter so we get a spread; assert every
        // sample lands within +/-25%. Loop count is small; the
        // LCG-ish shape is deterministic across a single thread
        // so we get reasonable distribution without flakes.
        for _ in 0..500 {
            let d = jitter(base);
            let ratio = d.as_millis() as f64 / base.as_millis() as f64;
            assert!(
                (0.75..=1.25).contains(&ratio),
                "jitter out of range: {d:?} (ratio {ratio:.3})"
            );
        }
    }
}
