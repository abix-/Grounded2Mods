//! 1Hz worker that watches a slot-key resolver closure and drives
//! activate / deactivate transitions on a consumer-provided pair of
//! callbacks.
//!
//! The slot-key resolver is game-specific (Grounded 2 reads
//! `AInGameGameState.PlaythroughGuid` at +0x32C; another UE game
//! might read a different field). The consumer plugs that closure
//! in; the poller handles transition detection.
//!
//! ```ignore
//! let handle = modforge::rpg::SlotPoller::spawn(
//!     std::time::Duration::from_secs(1),
//!     game::current_slot_key,        // -> Option<String>
//!     |slot| tracker::activate(slot),
//!     ||     tracker::deactivate(),
//! );
//!
//! // Later, from `on_shutdown`:
//! handle.stop();
//! ```
//!
//! Stop is graceful: the worker uses a `Condvar` keyed off the stop
//! flag so `stop()` wakes the thread immediately (no waiting up to
//! `interval` for the next poll). The handle stores a thread join
//! and `stop()` blocks until the thread has exited. Required for
//! hot-reload-safe shutdown.
//!
//! Spawned handles are also auto-registered into
//! [`POLLER_REGISTRY`]; the framework's shutdown sequence
//! ([`shutdown_all`]) stops every running poller without the
//! caller needing to thread its handle through.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::{Condvar, Mutex};

/// Handle to a running [`SlotPoller`]. Call [`Self::stop`] to
/// signal the worker, wake it from sleep, and join its thread.
/// Idempotent. Auto-called on drop.
pub struct PollerHandle {
    inner: Arc<HandleInner>,
}

struct HandleInner {
    stop: AtomicBool,
    wake: Condvar,
    wake_mu: Mutex<()>,
    panics: AtomicU64,
    last_panic: Mutex<Option<String>>,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl PollerHandle {
    /// Signal the worker to exit, wake it from sleep, and join
    /// the thread. Idempotent.
    pub fn stop(&self) {
        self.stop_soon();
        if let Some(j) = self.inner.join.lock().take() {
            let _ = j.join();
        }
    }

    /// Ask the worker to exit, without waiting for it.
    ///
    /// Safe to call FROM the worker's own tick, which [`stop`] is
    /// not: that one joins the thread, and a thread joining itself
    /// is a deadlock. A job that has finished for good ends itself
    /// with this.
    ///
    /// The loop checks the flag straight after each tick, so the
    /// thread is gone before the next interval rather than after
    /// it.
    pub fn stop_soon(&self) {
        self.inner.stop.store(true, Ordering::Release);
        // Wake the sleeping thread immediately.
        self.inner.wake.notify_all();
    }

    pub fn panic_count(&self) -> u64 {
        self.inner.panics.load(Ordering::Relaxed)
    }

    pub fn last_panic(&self) -> Option<String> {
        self.inner.last_panic.lock().clone()
    }
}

impl Drop for PollerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Process-global registry so the framework's shutdown sequence
/// can stop every running poller without threading individual
/// handles around. Populated by every `SlotPoller::spawn` call.
static POLLER_REGISTRY: Mutex<Vec<Arc<HandleInner>>> = Mutex::new(Vec::new());

/// Stop every running poller. Called from
/// [`crate::shutdown::SHUTDOWN_REGISTRY`] during
/// `unityforge_shutdown` / `ueforge_shutdown`.
pub fn shutdown_all() {
    let inners: Vec<Arc<HandleInner>> = {
        let mut g = POLLER_REGISTRY.lock();
        std::mem::take(&mut *g)
    };
    let n = inners.len();
    for inner in inners {
        inner.stop.store(true, Ordering::Release);
        inner.wake.notify_all();
        if let Some(j) = inner.join.lock().take() {
            let _ = j.join();
        }
    }
    if n > 0 {
        crate::log!("rpg/poller: shutdown_all stopped {n} poller(s)");
    }
}

/// Spawn a plain interval worker: run `tick` every `interval`
/// until stopped. Returns the same [`PollerHandle`] as
/// [`SlotPoller`] and joins the same registry, so
/// [`shutdown_all`] stops it with everything else.
///
/// Use this for ANY background loop in a mod. A raw
/// `thread::spawn(loop { sleep; work })` cannot be stopped, so
/// the thread keeps executing after the DLL unloads: that is
/// what makes hot reload fatal. A worker spawned here is woken
/// out of its sleep and joined during shutdown, so the image can
/// unload safely.
///
/// Panics inside `tick` are caught, counted, and logged; one bad
/// tick does not kill the loop.
pub fn spawn_interval<F>(name: &'static str, interval: Duration, tick: F) -> PollerHandle
where
    F: Fn() + Send + 'static,
{
    let inner = Arc::new(HandleInner {
        stop: AtomicBool::new(false),
        wake: Condvar::new(),
        wake_mu: Mutex::new(()),
        panics: AtomicU64::new(0),
        last_panic: Mutex::new(None),
        join: Mutex::new(None),
    });

    let thread_inner = inner.clone();
    let join = std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            loop {
                // Sleep first so a tick never runs before the
                // caller has finished wiring things up, and wake
                // immediately when shutdown signals.
                {
                    let mut guard = thread_inner.wake_mu.lock();
                    thread_inner.wake.wait_for(&mut guard, interval);
                }
                if thread_inner.stop.load(Ordering::Acquire) {
                    break;
                }
                // Every repeating job in every game goes through
                // here, and each one already has a name, so this
                // is the one place that measures them all. Costs
                // an atomic load when timing is off.
                let _m = crate::counters::measure(name);
                if let Err(e) = std::panic::catch_unwind(AssertUnwindSafe(&tick)) {
                    thread_inner.panics.fetch_add(1, Ordering::Relaxed);
                    let msg = panic_message(&e);
                    crate::log!("{name}: tick panicked: {msg}");
                    *thread_inner.last_panic.lock() = Some(msg);
                }
                // Again, because a tick that has finished for good
                // ends itself with `stop_soon` and should not wait
                // out another interval first.
                if thread_inner.stop.load(Ordering::Acquire) {
                    break;
                }
            }
            crate::log!("{name}: stopped");
        });

    match join {
        Ok(j) => {
            inner.join.lock().replace(j);
        }
        Err(e) => {
            crate::log!("{name}: spawn failed: {e}");
        }
    }

    POLLER_REGISTRY.lock().push(inner.clone());
    PollerHandle { inner }
}

fn panic_message(e: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = e.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = e.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

pub struct SlotPoller;

impl SlotPoller {
    /// Spawn the watcher on a named Rust thread. Returns a
    /// [`PollerHandle`] that can stop + join the worker.
    /// Panics in the resolver / callbacks are caught, logged,
    /// counted, and the most recent is exposed via
    /// [`PollerHandle::last_panic`]. A single bad tick doesn't
    /// kill slot tracking for the rest of the session.
    pub fn spawn<R, A, D>(
        interval: Duration,
        resolve: R,
        on_activate: A,
        on_deactivate: D,
    ) -> PollerHandle
    where
        R: Fn() -> Option<String> + Send + 'static,
        A: Fn(String) + Send + 'static,
        D: Fn() + Send + 'static,
    {
        let inner = Arc::new(HandleInner {
            stop: AtomicBool::new(false),
            wake: Condvar::new(),
            wake_mu: Mutex::new(()),
            panics: AtomicU64::new(0),
            last_panic: Mutex::new(None),
            join: Mutex::new(None),
        });

        let thread_inner = inner.clone();
        let join = std::thread::Builder::new()
            .name("modforge/rpg/slot-poller".to_string())
            .spawn(move || {
                run(Cfg {
                    interval,
                    resolve: Box::new(resolve),
                    on_activate: Box::new(on_activate),
                    on_deactivate: Box::new(on_deactivate),
                    inner: thread_inner,
                });
            });

        match join {
            Ok(j) => {
                inner.join.lock().replace(j);
            }
            Err(e) => {
                crate::log!("rpg/poller: spawn failed: {e}");
            }
        }

        POLLER_REGISTRY.lock().push(inner.clone());

        PollerHandle { inner }
    }
}

struct Cfg {
    interval: Duration,
    resolve: Box<dyn Fn() -> Option<String> + Send + 'static>,
    on_activate: Box<dyn Fn(String) + Send + 'static>,
    on_deactivate: Box<dyn Fn() + Send + 'static>,
    inner: Arc<HandleInner>,
}

fn run(cfg: Cfg) {
    crate::log!("rpg/poller: started, interval={:?}", cfg.interval);
    let mut last: Option<String> = None;

    while !cfg.inner.stop.load(Ordering::Acquire) {
        let cur_result = std::panic::catch_unwind(AssertUnwindSafe(|| (cfg.resolve)()));
        let cur = match cur_result {
            Ok(v) => v,
            Err(payload) => {
                record_panic(&cfg, "resolve", &payload);
                None
            }
        };

        match (last.as_deref(), cur.as_deref()) {
            (None, Some(s)) => fire(&cfg, "on_activate", || (cfg.on_activate)(s.to_string())),
            (Some(a), Some(b)) if a != b => {
                fire(&cfg, "on_deactivate", || (cfg.on_deactivate)());
                fire(&cfg, "on_activate", || (cfg.on_activate)(b.to_string()));
            }
            (Some(_), None) => fire(&cfg, "on_deactivate", || (cfg.on_deactivate)()),
            _ => {}
        }
        last = cur;

        // Cond-var sleep: wake immediately on stop, otherwise
        // wait at most `interval`. wait_for releases the mutex
        // while waiting and re-acquires on wake; we don't need
        // the mutex for any real shared state, only for the
        // condvar's contract.
        let mut g = cfg.inner.wake_mu.lock();
        if cfg.inner.stop.load(Ordering::Acquire) {
            break;
        }
        let _ = cfg.inner.wake.wait_for(&mut g, cfg.interval);
    }

    crate::log!("rpg/poller: stop signal received, exiting");
}

fn fire(cfg: &Cfg, kind: &str, work: impl FnOnce()) {
    if let Err(payload) = std::panic::catch_unwind(AssertUnwindSafe(work)) {
        record_panic(cfg, kind, &payload);
    }
}

fn record_panic(cfg: &Cfg, kind: &str, payload: &Box<dyn std::any::Any + Send>) {
    cfg.inner.panics.fetch_add(1, Ordering::Relaxed);
    let msg = if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    };
    crate::log!("rpg/poller: panic in {kind}: {msg}");
    *cfg.inner.last_panic.lock() = Some(format!("[{kind}] {msg}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn handle_stop_flips_flag() {
        let h = SlotPoller::spawn(
            Duration::from_millis(10),
            || None,
            |_| {},
            || {},
        );
        std::thread::sleep(Duration::from_millis(50));
        h.stop();
        assert!(h.inner.stop.load(Ordering::Acquire));
    }

    #[test]
    fn stop_wakes_immediately() {
        // Long interval; without condvar wake this test would
        // block for the full duration. With it, stop should
        // return in ~10ms.
        let h = SlotPoller::spawn(
            Duration::from_secs(60),
            || None,
            |_| {},
            || {},
        );
        std::thread::sleep(Duration::from_millis(50));
        let t0 = std::time::Instant::now();
        h.stop();
        let elapsed = t0.elapsed();
        assert!(
            elapsed < Duration::from_secs(1),
            "stop took {elapsed:?}; condvar wake didn't fire"
        );
    }

    #[test]
    fn resolver_panic_is_caught_and_counted() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_for_resolver = counter.clone();
        let h = SlotPoller::spawn(
            Duration::from_millis(10),
            move || {
                let n = counter_for_resolver.fetch_add(1, Ordering::Relaxed);
                if n == 0 {
                    panic!("intentional resolver panic");
                }
                None
            },
            |_| {},
            || {},
        );
        std::thread::sleep(Duration::from_millis(100));
        h.stop();
        assert!(h.panic_count() >= 1, "panics={}", h.panic_count());
        assert!(h.last_panic().is_some());
        assert!(counter.load(Ordering::Relaxed) >= 2,
            "thread should have kept running after panic");
    }

    #[test]
    fn no_panics_means_clean_handle() {
        let h = SlotPoller::spawn(
            Duration::from_millis(10),
            || None,
            |_| {},
            || {},
        );
        std::thread::sleep(Duration::from_millis(50));
        h.stop();
        assert_eq!(h.panic_count(), 0);
        assert!(h.last_panic().is_none());
    }
}
