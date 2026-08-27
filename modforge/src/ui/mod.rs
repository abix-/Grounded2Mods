//! Engine-agnostic UI declarative shape.
//!
//! Game mods describe their UI tabs declaratively. Each engine
//! framework renders them with whatever ImGui binding it ships
//! (UE4SS via C++ shim for ueforge, an in-process ImGui or
//! Unity OnGUI for unityforge).
//!
//! ```ignore
//! pub static MOD_INFO: ModDef = ModDef {
//!     name: "MyMod",
//!     // ...
//!     tabs: &[
//!         TabDef { name: "RPG",      render: render_rpg },
//!         TabDef { name: "Debug",    render: render_debug },
//!     ],
//! };
//! ```
//!
//! Rendering implementations stay engine-specific; only the
//! declarative shape is shared.

/// One UI tab declaration. The renderer is a bare `fn()` (no
/// captures, no state) so the struct is `Copy`-able and can
/// live in a `'static` slice.
pub struct TabDef {
    pub name: &'static str,
    pub render: fn(),
}

#[cfg(feature = "overlay-ui")]
pub mod overlay;

/// A value re-read no more often than you say.
///
/// A tab's render runs every frame while it is open. Reading live
/// game state there means doing that work sixty times a second to
/// draw a number a player reads once. In MISERY that meant a full
/// object search per frame, 100 ms of it, for a tab that shows
/// eight numbers.
///
/// Hold one in a `static`, ask for the value with a maximum age,
/// and the closure runs only when the last answer is older than
/// that.
///
/// ```ignore
/// static SPEEDS: Cached<Vec<Entry>> = Cached::new();
///
/// pub fn render() {
///     let speeds = SPEEDS.get(Duration::from_secs(1), read_them);
///     // ... draw ...
///     if ui::button("Refresh") { SPEEDS.invalidate() }
/// }
/// ```
pub struct Cached<T> {
    inner: parking_lot::Mutex<Option<(std::time::Instant, T)>>,
}

impl<T: Clone> Cached<T> {
    pub const fn new() -> Self {
        Self { inner: parking_lot::Mutex::new(None) }
    }

    /// The value, re-read if the last one is older than
    /// `max_age`, or if nothing has been read yet.
    pub fn get(&self, max_age: std::time::Duration, compute: impl FnOnce() -> T) -> T {
        let mut slot = self.inner.lock();
        let stale = match slot.as_ref() {
            Some((at, _)) => at.elapsed() >= max_age,
            None => true,
        };
        if stale {
            *slot = Some((std::time::Instant::now(), compute()));
        }
        // Just refreshed, or was fresh already.
        slot.as_ref().map(|(_, v)| v.clone()).expect("just filled")
    }

    /// Throw the last answer away, so the next `get` re-reads. For
    /// a Refresh button, and for after something was changed.
    pub fn invalidate(&self) {
        *self.inner.lock() = None;
    }
}

impl<T: Clone> Default for Cached<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod cached_tests {
    use super::Cached;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    #[test]
    fn reads_once_then_reuses() {
        let reads = AtomicU32::new(0);
        let c: Cached<u32> = Cached::new();
        for _ in 0..10 {
            c.get(Duration::from_secs(60), || {
                reads.fetch_add(1, Ordering::Relaxed);
                7
            });
        }
        assert_eq!(reads.load(Ordering::Relaxed), 1, "should have read once");
    }

    #[test]
    fn re_reads_once_it_is_old_enough() {
        let reads = AtomicU32::new(0);
        let c: Cached<u32> = Cached::new();
        let mut go = || {
            c.get(Duration::from_millis(10), || {
                reads.fetch_add(1, Ordering::Relaxed);
                1
            })
        };
        go();
        std::thread::sleep(Duration::from_millis(20));
        go();
        assert_eq!(reads.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn invalidate_forces_a_re_read() {
        let reads = AtomicU32::new(0);
        let c: Cached<u32> = Cached::new();
        c.get(Duration::from_secs(60), || {
            reads.fetch_add(1, Ordering::Relaxed);
            1
        });
        c.invalidate();
        c.get(Duration::from_secs(60), || {
            reads.fetch_add(1, Ordering::Relaxed);
            1
        });
        assert_eq!(reads.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn the_value_comes_back_unchanged() {
        let c: Cached<String> = Cached::new();
        let got = c.get(Duration::from_secs(60), || "hello".to_string());
        assert_eq!(got, "hello");
        let again = c.get(Duration::from_secs(60), || "different".to_string());
        assert_eq!(again, "hello", "should have reused the first answer");
    }
}
