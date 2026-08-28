//! Persistent mod settings backed by a JSON file under the
//! mod's DLL directory.
//!
//! Generic over the embedding crate's settings struct (must be
//! `Serialize + Deserialize + Default + Clone`). Load on
//! construction, atomically save on every mutation. Missing
//! file -> defaults; parse errors -> defaults + a log line.
//!
//! ```ignore
//! #[derive(Serialize, Deserialize, Default, Clone)]
//! struct TweaksSettings {
//!     #[serde(default)]
//!     stacks: StacksSettings,
//! }
//!
//! static SETTINGS: OnceLock<Settings<TweaksSettings>> = OnceLock::new();
//! fn settings() -> &'static Settings<TweaksSettings> {
//!     SETTINGS.get_or_init(|| Settings::load("tweaks.json"))
//! }
//!
//! // Read:
//! let mult = settings().get().stacks.multiplier;
//!
//! // Write (auto-saves to disk):
//! settings().update(|s| s.stacks.multiplier = 8);
//! ```
//!
//! Threading: mutations and reads are mutex-guarded. The save
//! step happens inside the mutex, so concurrent updates serialize
//! cleanly. Every update writes the entire struct via a temp file
//! + rename for atomicity (POSIX semantics; on Windows
//! `std::fs::rename` over an existing file is also atomic since
//! 1809).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, SystemTime};

use parking_lot::Mutex;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Global registry of spawned `Settings::watch` worker threads.
/// `Settings::watch` registers each spawn so [`shutdown_all`] can
/// stop and join them on hot-reload teardown. The previous
/// design relied on the caller holding the `WatchHandle` and
/// dropping it before unload, which silently leaked the thread
/// across DLL unloads when the caller forgot.
struct WatchEntry {
    stop: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}

static WATCH_REGISTRY: Mutex<Vec<WatchEntry>> = Mutex::new(Vec::new());

/// Stop and join every registered settings watcher. Called by the
/// framework's hot-reload teardown path
/// ([`crate::mod_main::ueforge_mod_shutdown`]) so no watcher
/// thread is on a stack inside our DLL when UE4SS calls
/// `FreeLibrary`.
pub fn shutdown_all() {
    let entries: Vec<WatchEntry> = std::mem::take(&mut *WATCH_REGISTRY.lock());
    let n = entries.len();
    for mut e in entries {
        e.stop.store(true, Ordering::Release);
        if let Some(j) = e.join.take() {
            let _ = j.join();
        }
    }
    if n > 0 {
        crate::log!("settings: shutdown_all stopped {n} watcher(s)");
    }
}

/// Handle returned by [`Settings::watch`]. Holding it keeps the
/// hot-reload poller running; dropping it asks the poller to stop
/// at its next tick.
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
}

impl WatchHandle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct Settings<T> {
    inner: Mutex<T>,
    path: PathBuf,
}

impl<T> Settings<T>
where
    T: Serialize + DeserializeOwned + Default + Clone,
{
    /// Load `<DLL_dir>/<file_name>` into memory. Missing or
    /// unparseable file falls back to `T::default()` with a log
    /// line. The path is captured for subsequent `update` /
    /// `save` calls.
    pub fn load(file_name: &str) -> Self {
        let path = match crate::log::dll_dir() {
            Some(d) => d.join(file_name),
            None => PathBuf::from(file_name),
        };
        let initial = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<T>(&text) {
                Ok(v) => {
                    crate::log::log(format_args!("settings: loaded {}", path.display()));
                    v
                }
                Err(e) => {
                    crate::log::log(format_args!(
                        "settings: parse error in {}: {} -- using defaults",
                        path.display(),
                        e
                    ));
                    T::default()
                }
            },
            Err(_) => {
                crate::log::log(format_args!(
                    "settings: {} not found, using defaults",
                    path.display()
                ));
                T::default()
            }
        };
        Self {
            inner: Mutex::new(initial),
            path,
        }
    }

    /// Snapshot the current settings. Cheap clone; safe to call
    /// from any thread.
    pub fn get(&self) -> T {
        self.inner.lock().clone()
    }

    /// Mutate settings and save the new state to disk. The
    /// closure runs under the lock; keep it short. Returns the
    /// mutated value.
    ///
    /// Save errors are logged but not surfaced. Settings
    /// persistence is best-effort and shouldn't break gameplay
    /// callsites.
    pub fn update<F>(&self, f: F) -> T
    where
        F: FnOnce(&mut T),
    {
        let mut g = self.inner.lock();
        f(&mut *g);
        let snap = g.clone();
        if let Err(e) = save_atomic(&self.path, &*g) {
            crate::log::log(format_args!(
                "settings: save to {} failed: {}",
                self.path.display(),
                e
            ));
        }
        snap
    }

    /// Force a save without mutating. Useful when settings were
    /// updated via raw `set` / direct field access from a
    /// trusted callsite.
    pub fn save(&self) -> std::io::Result<()> {
        let g = self.inner.lock();
        save_atomic(&self.path, &*g)
    }

    /// Path of the backing file. Useful for diagnostic logging.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Re-read the backing file from disk and replace the in-memory
    /// state with whatever it deserializes to. Returns the new
    /// value on success, or an error string + leaves the in-memory
    /// state untouched on parse / IO failure.
    ///
    /// Used by [`watch`](Self::watch) when an mtime change is
    /// detected; also callable directly from a debug-endpoint op
    /// or a tab button.
    pub fn reload(&self) -> Result<T, String> {
        let text = std::fs::read_to_string(&self.path)
            .map_err(|e| format!("read {}: {e}", self.path.display()))?;
        let parsed: T = serde_json::from_str(&text)
            .map_err(|e| format!("parse {}: {e}", self.path.display()))?;
        let snap = parsed.clone();
        *self.inner.lock() = parsed;
        Ok(snap)
    }
}

impl<T> Settings<T>
where
    T: Serialize + DeserializeOwned + Default + Clone + Send + 'static,
{
    /// Spawn a background thread that polls the backing file's
    /// mtime every `interval` and reloads when it changes.
    /// `on_reload` fires after each successful reload with the
    /// new value (the game crate uses this to push the new
    /// settings into per-feature atomics / re-apply CDO writes).
    ///
    /// Drop the returned [`WatchHandle`] to stop the poller.
    /// The handle's `stop()` is checked once per tick, so the
    /// thread exits within `interval` of the drop.
    ///
    /// Parse errors are logged and skipped. The live in-memory
    /// state is untouched. Subsequent successful reloads recover.
    pub fn watch<F>(self: &Arc<Self>, interval: Duration, on_reload: F) -> WatchHandle
    where
        F: Fn(&T) + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let this = self.clone();
        let path = self.path.clone();

        let join = std::thread::Builder::new()
            .name("ueforge-settings-watch".into())
            .spawn(move || {
                let mut last_mtime: Option<SystemTime> =
                    std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                loop {
                    if stop_clone.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(interval);
                    if stop_clone.load(Ordering::Relaxed) {
                        return;
                    }
                    let cur_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                    if cur_mtime == last_mtime {
                        continue;
                    }
                    last_mtime = cur_mtime;
                    match this.reload() {
                        Ok(snap) => {
                            crate::log::log(format_args!(
                                "settings: hot-reloaded {}",
                                path.display()
                            ));
                            on_reload(&snap);
                        }
                        Err(e) => {
                            crate::log::log(format_args!("settings: hot-reload skipped: {e}"));
                        }
                    }
                }
            })
            .expect("spawn settings watcher");

        // Register so hot-reload teardown can join the thread even
        // if the caller forgot to keep the WatchHandle around.
        WATCH_REGISTRY.lock().push(WatchEntry {
            stop: stop.clone(),
            join: Some(join),
        });

        WatchHandle { stop }
    }
}

fn save_atomic<T: Serialize>(path: &std::path::Path, value: &T) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| std::io::Error::other(format!("serialize: {e}")))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
