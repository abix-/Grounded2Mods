//! Which streamed levels are loaded, without reading the world.
//!
//! A mod that reacts to level streaming has to know which levels
//! are up. The obvious way is to search the object list for
//! actors and read the level out of each one's path. That way
//! costs, measured in MISERY 2026-08-26, **174,000 to 230,000
//! objects and 94 to 132 ms per search**, on the game thread,
//! which is six to eight frames. The price grows with the world.
//!
//! The game already keeps the list. A level streamer object holds
//! an array of `ULevelStreaming*`, and each of those points at
//! its loaded `ULevel`, whose name IS the level. Reading that is
//! a cached pointer and two array reads.
//!
//! ```text
//! streamer           found ONCE, pointer cached
//!   -> levels array  a few entries
//!   -> loaded level  one pointer per entry
//!   -> its name      the level
//! ```
//!
//! What differs per game is the streamer's class and two offsets,
//! which the consumer supplies as [`LevelStreamer`]. Nothing here
//! knows a game.
//!
//! The point is not that this is faster. It is that a check with
//! nothing to report does no searching at all, so standing still
//! costs nothing.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::ue::{self, UObject, read_at};

/// Where a game keeps its streamed levels.
///
/// `class` is the object holding the array; `levels` is the byte
/// offset of that `TArray<ULevelStreaming*>`; `loaded_level` is
/// the byte offset, on each entry, of the `ULevel` it has loaded.
///
/// Measure the offsets, never guess them.
/// `misery-mod/tests/research_streaming.rs` shows how, by
/// comparing addresses against a known list rather than chasing
/// pointers found in memory. Chasing them killed the game three
/// times in one evening.
#[derive(Clone, Copy)]
pub struct LevelStreamer {
    pub class: &'static str,
    pub levels: usize,
    pub loaded_level: usize,
}

/// A UE `TArray` header: `{ void* Data; int32 Num; int32 Max; }`.
const TARRAY_NUM: usize = 8;

/// Refuse to walk an array longer than this. A garbage length
/// read out of a stale object would otherwise send us reading
/// through memory that is not ours. Streamed level counts are in
/// the tens.
const SANE_MAX: i32 = 4096;

/// The cached streamer pointer.
///
/// Measured stable: read twice ten seconds apart with the player
/// moving, the addresses were identical, so the search that finds
/// it happens once per session rather than on every check.
static STREAMER: AtomicUsize = AtomicUsize::new(0);

impl LevelStreamer {
    /// The names of the levels this streamer has loaded.
    ///
    /// No object search. Game thread only, like anything that
    /// touches a live object.
    ///
    /// Empty when the streamer has not been found yet or has
    /// nothing streamed. A caller cannot tell those apart, and
    /// should not need to: both mean there is nothing to react
    /// to.
    pub fn loaded_levels(&self) -> Vec<String> {
        let Some(streamer) = self.streamer() else {
            return Vec::new();
        };
        let ptr = streamer as *const UObject as *const u8;
        // SAFETY: `streamer` came from a GObjects search on this
        // thread, and the offsets are the consumer's measured
        // ones.
        let (data, num) = unsafe {
            (
                read_at::<u64>(ptr, self.levels),
                read_at::<i32>(ptr, self.levels + TARRAY_NUM),
            )
        };
        if data == 0 || num <= 0 || num > SANE_MAX {
            return Vec::new();
        }

        let mut out = Vec::with_capacity(num as usize);
        for i in 0..num as usize {
            // SAFETY: within the array the game itself reports.
            let entry = unsafe { read_at::<u64>(data as *const u8, i * 8) };
            if entry == 0 {
                continue;
            }
            let level =
                // SAFETY: `entry` is an element of the engine's
                // own array of level-streaming objects.
                unsafe { read_at::<u64>(entry as *const u8, self.loaded_level) };
            if level == 0 {
                // Streaming in, or already out. Not loaded.
                continue;
            }
            // SAFETY: the engine's own loaded-level pointer.
            let obj = unsafe { &*(level as *const UObject) };
            out.push(obj.full_name());
        }
        out
    }

    /// The streamer object, found once and remembered.
    ///
    /// The one search this module ever does, and only until it
    /// succeeds. If the remembered object stops reporting a sane
    /// array the pointer is dropped and found again, which covers
    /// the streamer being replaced across a level load.
    fn streamer(&self) -> Option<&'static UObject> {
        let cached = STREAMER.load(Ordering::Relaxed);
        if cached != 0 {
            // SAFETY: stored by this function from a GObjects
            // search; validated below before it is trusted.
            let obj = unsafe { &*(cached as *const UObject) };
            if self.array_looks_sane(obj) {
                return Some(obj);
            }
            STREAMER.store(0, Ordering::Relaxed);
        }
        // Several streamers can exist, one per area in MISERY's
        // case. The one that matters is the one actually
        // streaming something.
        let found = ue::actor::find_actors_by_chain(self.class)
            .into_iter()
            .map(|p| {
                // SAFETY: p came from that call's own GObjects
                // iteration.
                unsafe { &*(p as *const UObject) }
            })
            .find(|obj| self.array_looks_sane(obj))?;
        STREAMER.store(found as *const UObject as usize, Ordering::Relaxed);
        Some(found)
    }

    /// Does this object hold a non-empty, plausible array?
    ///
    /// Doubles as the check that a remembered pointer is still
    /// worth using, and as the way to pick the active streamer
    /// out of several.
    fn array_looks_sane(&self, obj: &UObject) -> bool {
        let ptr = obj as *const UObject as *const u8;
        // SAFETY: reading two fields at the consumer's measured
        // offsets on an object it named.
        let (data, num) = unsafe {
            (
                read_at::<u64>(ptr, self.levels),
                read_at::<i32>(ptr, self.levels + TARRAY_NUM),
            )
        };
        data != 0 && num > 0 && num <= SANE_MAX
    }

    /// Forget the remembered streamer. For a consumer that knows
    /// the world has gone, rather than waiting to notice.
    pub fn forget(&self) {
        STREAMER.store(0, Ordering::Relaxed);
    }
}

/// The streamer this mod registered, if any.
static REGISTERED: parking_lot::Mutex<Option<LevelStreamer>> = parking_lot::Mutex::new(None);

/// Tell the framework where this game keeps its streamed levels.
///
/// Call once at init. Anything in ueforge that needs to know
/// whether a world is up then gets a cheap answer instead of
/// searching for one: see [`world_is_up`].
pub fn register(streamer: LevelStreamer) {
    *REGISTERED.lock() = Some(streamer);
}

/// Is a world loaded?
///
/// Cheap: a cached pointer and an array length, no object search.
/// `None` when no streamer has been registered, which means "no
/// idea" rather than "no world", so a caller must fall back to
/// whatever it did before rather than assume.
///
/// Game thread only.
///
/// This is the bit a load-watcher actually needs. Watching for a
/// world to go away by re-running an expensive search and seeing
/// nothing costs 100 ms to learn one bit, and MISERY was paying
/// it every three seconds for the life of the process
/// (`misery-mod/docs/performance.md`).
pub fn world_is_up() -> Option<bool> {
    let streamer = *REGISTERED.lock();
    streamer.map(|s| !s.loaded_levels().is_empty())
}
