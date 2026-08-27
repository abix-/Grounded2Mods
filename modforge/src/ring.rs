//! Bounded drop-oldest ring buffer for capturing hook events.
//!
//! Pattern: a hook trampoline (damage multicast, tick, anything
//! firing many times per second) calls `Ring::push(event)`. The
//! ring keeps at most `capacity` items, dropping the oldest on
//! overflow so a long session can't exhaust memory. The
//! `/debug snapshot` op clones the contents into the response.
//!
//! Mutex-protected (`parking_lot::Mutex`, no poisoning). Push is
//! rare relative to gameplay; clone is once per snapshot request
//! (low rate). If contention ever shows up, swap to a lock-free
//! SPSC and a per-frame drain.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

pub struct Ring<T> {
    inner: Mutex<Vec<T>>,
    capacity: usize,
}

impl<T> Ring<T> {
    pub const fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(Vec::new()),
            capacity,
        }
    }

    /// Append an item. If the ring is at capacity, the oldest
    /// item is dropped first. Returns the new length.
    pub fn push(&self, item: T) -> usize {
        let mut g = self.inner.lock();
        g.push(item);
        let n = g.len();
        if n > self.capacity {
            g.drain(0..n - self.capacity);
        }
        g.len()
    }

    /// Snapshot a copy of the ring contents.
    pub fn snapshot(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.inner.lock().clone()
    }

    /// Snapshot the current contents and clear them under the
    /// same lock, so concurrent producers cannot lose a new item
    /// between the snapshot and clear operations.
    pub fn snapshot_and_clear(&self) -> Vec<T>
    where
        T: Clone,
    {
        let mut inner = self.inner.lock();
        let snapshot = inner.clone();
        inner.clear();
        snapshot
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

struct RecentRingInner<K: Copy, const N: usize> {
    entries: [Option<(K, Instant)>; N],
    next: usize,
}

/// Fixed-capacity timestamped keys for recent-event attribution
/// and duplicate suppression.
pub struct RecentRing<K: Copy, const N: usize> {
    inner: Mutex<RecentRingInner<K, N>>,
}

impl<K: Copy + Eq, const N: usize> RecentRing<K, N> {
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(RecentRingInner {
                entries: [None; N],
                next: 0,
            }),
        }
    }

    /// Record a key at the current time, replacing the oldest
    /// entry when the ring is full.
    pub fn remember(&self, key: K) {
        let mut inner = self.inner.lock();
        let next = inner.next;
        inner.entries[next] = Some((key, Instant::now()));
        inner.next = (next + 1) % N;
    }

    /// Return whether `key` was recorded within `window`.
    pub fn recent(&self, key: K, window: Duration) -> bool {
        let now = Instant::now();
        self.inner.lock().entries.iter().any(|entry| {
            entry.is_some_and(|(recorded, at)| {
                recorded == key && now.duration_since(at) < window
            })
        })
    }
}

/// `Ring<T>` + automatic push-counter + peak high-watermark tracking.
///
/// Replaces the boilerplate of pairing a [`Ring`] with two
/// statics (`PUSHES`, `PEAK`) and remembering to bump them in the
/// `record_event` wrapper. Use one `EventRing` per structured-event
/// surface (damage trace, kill log, etc.) and the snapshot endpoint
/// gets the events; the perf-counter snapshot gets the push count
/// + peak depth automatically.
///
/// ```ignore
/// static DAMAGE: ueforge::ring::EventRing<DamageEvent> =
///     ueforge::ring::EventRing::new(64);
///
/// // From the trampoline:
/// DAMAGE.record(DamageEvent { ... });
///
/// // From the snapshot endpoint:
/// let events = DAMAGE.snapshot();
/// let pushes = DAMAGE.pushes();
/// let peak = DAMAGE.peak();
/// ```
pub struct EventRing<T> {
    ring: Ring<T>,
    pushes: AtomicU64,
    peak: AtomicUsize,
}

impl<T> EventRing<T> {
    pub const fn new(capacity: usize) -> Self {
        Self {
            ring: Ring::new(capacity),
            pushes: AtomicU64::new(0),
            peak: AtomicUsize::new(0),
        }
    }

    /// Push an event, bumping the push counter and updating the
    /// peak high-watermark. Equivalent to g2rpg's old
    /// `record_damage_event` wrapper minus the per-mod statics.
    pub fn record(&self, item: T) {
        self.pushes.fetch_add(1, Ordering::Relaxed);
        let n = self.ring.push(item);
        crate::counters::observe_peak(&self.peak, n);
    }

    pub fn snapshot(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.ring.snapshot()
    }

    pub fn len(&self) -> usize {
        self.ring.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.ring.capacity()
    }

    /// Total `record` calls since process start. For the snapshot
    /// `counters` block.
    pub fn pushes(&self) -> u64 {
        self.pushes.load(Ordering::Relaxed)
    }

    /// Peak ring length observed. For the snapshot `counters` block.
    pub fn peak(&self) -> usize {
        self.peak.load(Ordering::Relaxed)
    }
}
