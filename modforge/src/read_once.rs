//! Work something out once, keep it, and throw it away when an
//! event says so.
//!
//! The shape almost everything a mod reads out of a game takes.
//! Where the player is. What a manager's fields say. Which speeds
//! are set. None of it changes on a clock; it changes when the
//! game changes it, and most of the time nothing changes it at
//! all.
//!
//! **Deliberately not a timer.** Re-reading every N seconds is
//! polling with a longer gap. Every interval is a guess, and the
//! right interval for something that changes on an event is
//! never. So: read on first use, and [`forget`] when the event
//! happens.
//!
//! ```ignore
//! static SPEEDS: ReadOnce<Vec<Entry>> = ReadOnce::new();
//!
//! // in the tab, every frame, costing nothing after the first:
//! let Some(speeds) = SPEEDS.get(read_them) else { return };
//!
//! // when we change them:
//! SPEEDS.forget();
//! ```
//!
//! Every `ReadOnce` that has been used registers itself, so one
//! [`forget_all`] clears the lot. In an Unreal mod that is wired
//! to the world ending, which is the one event that invalidates
//! anything read out of a world.
//!
//! Measured reason this exists, MISERY 2026-08-26: a tab that
//! read live state on every frame it was open cost a full object
//! search per frame, about 20 ms, sixty times a second, to draw
//! eight numbers that only this mod ever changed
//! (`misery-mod/docs/performance.md`).

use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::Mutex;

/// A value worked out once and kept until forgotten.
pub struct ReadOnce<T: 'static> {
    value: Mutex<Option<T>>,
    /// Whether this one is in [`ALL`] yet.
    listed: AtomicUsize,
}

/// Every `ReadOnce` that has been used, as a function that
/// clears it. A trait object would need the type; a closure does
/// not.
static ALL: Mutex<Vec<fn()>> = Mutex::new(Vec::new());

impl<T: Clone + 'static> ReadOnce<T> {
    pub const fn new() -> Self {
        Self {
            value: Mutex::new(None),
            listed: AtomicUsize::new(0),
        }
    }

    /// The value, working it out only if it is not already known.
    ///
    /// `None` when `compute` says there is nothing to read yet,
    /// and nothing is remembered in that case, so the next call
    /// tries again. That is what makes it safe to call before a
    /// world exists.
    pub fn get(&self, compute: impl FnOnce() -> Option<T>) -> Option<T> {
        let mut slot = self.value.lock();
        if slot.is_none() {
            *slot = compute();
        }
        slot.clone()
    }

    /// Forget it, so the next `get` works it out again.
    pub fn forget(&self) {
        *self.value.lock() = None;
    }

    /// Is anything remembered right now?
    pub fn is_held(&self) -> bool {
        self.value.lock().is_some()
    }

    /// Add this one to the list [`forget_all`] clears.
    ///
    /// Takes the clearing function rather than the value, because
    /// a list of differently-typed values cannot be held without
    /// naming every type.
    ///
    /// ```ignore
    /// static SPEEDS: ReadOnce<Vec<Entry>> = ReadOnce::new();
    /// SPEEDS.forget_with(|| SPEEDS.forget());
    /// ```
    pub fn forget_with(&self, clear: fn()) {
        if self.listed.swap(1, Ordering::AcqRel) == 0 {
            ALL.lock().push(clear);
        }
    }
}

impl<T: Clone + 'static> Default for ReadOnce<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Forget everything that registered with [`ReadOnce::forget_with`].
///
/// For the one event that invalidates everything read out of a
/// world: that world ending.
pub fn forget_all() {
    for clear in ALL.lock().iter() {
        clear();
    }
}

/// How many are registered. For diagnostics and tests.
pub fn registered() -> usize {
    ALL.lock().len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn works_it_out_once_then_keeps_it() {
        static READS: AtomicU32 = AtomicU32::new(0);
        let once: ReadOnce<u32> = ReadOnce::new();
        for _ in 0..10 {
            once.get(|| {
                READS.fetch_add(1, Ordering::Relaxed);
                Some(7)
            });
        }
        assert_eq!(READS.load(Ordering::Relaxed), 1);
        assert_eq!(once.get(|| Some(0)), Some(7));
    }

    /// Nothing to read yet is not an answer worth keeping, or the
    /// tab would say "no player" forever.
    #[test]
    fn nothing_yet_is_not_remembered() {
        static READS: AtomicU32 = AtomicU32::new(0);
        let once: ReadOnce<u32> = ReadOnce::new();
        for _ in 0..3 {
            once.get(|| {
                READS.fetch_add(1, Ordering::Relaxed);
                None
            });
        }
        assert_eq!(READS.load(Ordering::Relaxed), 3, "should keep trying");
        assert!(!once.is_held());
    }

    #[test]
    fn forget_makes_it_read_again() {
        static READS: AtomicU32 = AtomicU32::new(0);
        let once: ReadOnce<u32> = ReadOnce::new();
        once.get(|| {
            READS.fetch_add(1, Ordering::Relaxed);
            Some(1)
        });
        once.forget();
        once.get(|| {
            READS.fetch_add(1, Ordering::Relaxed);
            Some(2)
        });
        assert_eq!(READS.load(Ordering::Relaxed), 2);
        assert_eq!(once.get(|| Some(0)), Some(2), "the second answer stuck");
    }

    #[test]
    fn forget_all_clears_the_registered_ones() {
        static ONE: ReadOnce<u32> = ReadOnce::new();
        ONE.get(|| Some(5));
        ONE.forget_with(|| ONE.forget());
        assert!(ONE.is_held());
        forget_all();
        assert!(!ONE.is_held());
    }
}
