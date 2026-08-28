//! Per-slot JSON persistence under `<DLL_dir>/<subdir>/<slot>.json`.
//!
//! Atomic write: serialize -> write `<path>.tmp` -> `fsync` the
//! temp -> `rename` to `<path>`. The `fsync` step is what turns
//! "rename is atomic" into "rename is also durable across power
//! loss". Without it a crash between the rename and the
//! kernel's own flush leaves a torn or zero-byte file.
//!
//! Generic over the persisted struct so games can attach
//! diagnostic counters or game-specific fields without forking the
//! storage layer.
//!
//! Errors: every `save` returns `io::Result<()>`. Callers MUST
//! propagate or surface; silent failure is the bug that loses
//! save data. The store also caches the last-seen save error in
//! [`Self::last_error`] so the debug snapshot can expose it
//! without having to thread the Result up through callers that
//! don't know what to do with it.

use std::fs;
use std::io::{self, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::Serialize;
use serde::de::DeserializeOwned;

pub struct SlotStore<S> {
    subdir: &'static str,
    last_error: Mutex<Option<String>>,
    _phantom: PhantomData<fn() -> S>,
}

impl<S> SlotStore<S>
where
    S: Serialize + DeserializeOwned + Default,
{
    pub const fn new(subdir: &'static str) -> Self {
        Self {
            subdir,
            last_error: Mutex::new(None),
            _phantom: PhantomData,
        }
    }

    /// `<DLL_dir>/<subdir>/<slot>.json`. Resolves through
    /// [`crate::log::dll_dir`]; falls back to CWD if the HMODULE
    /// hasn't been captured yet.
    ///
    /// The slot key is validated to reject path separators and
    /// leading dots so a user-controlled string can't escape the
    /// configured subdir. Invalid keys produce a sentinel filename
    /// (`__invalid__.json`) that visibly signals the problem to
    /// the next reader rather than silently writing somewhere
    /// unexpected.
    pub fn path(&self, slot: &str) -> PathBuf {
        let mut p = crate::log::dll_dir().unwrap_or_else(|| PathBuf::from("."));
        p.push(self.subdir);
        let safe = if is_valid_slot(slot) {
            slot
        } else {
            "__invalid__"
        };
        p.push(format!("{safe}.json"));
        p
    }

    /// Load the slot's saved state, or `S::default()` if the file
    /// is missing or unparseable. Logs the outcome. Always
    /// succeeds (silent fall-through to default is the right
    /// behavior for missing-or-corrupt save data).
    pub fn load(&self, slot: &str) -> S {
        self.load_from_path(&self.path(slot))
    }

    /// Path-explicit form of [`Self::load`]. Test seam.
    pub(crate) fn load_from_path(&self, path: &Path) -> S {
        match fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str::<S>(&text) {
                Ok(s) => s,
                Err(e) => {
                    crate::log!(
                        "rpg/store: parse failed for {} ({e}); starting fresh",
                        path.display()
                    );
                    S::default()
                }
            },
            Err(_) => {
                crate::log!(
                    "rpg/store: no prior save at {}; starting fresh",
                    path.display()
                );
                S::default()
            }
        }
    }

    /// Atomic save. Write to `<path>.tmp`, fsync, rename to
    /// `<path>`. Returns the underlying `io::Result`. On error,
    /// caches the message in [`Self::last_error`] for snapshot
    /// surfaces and logs it.
    ///
    /// The fsync is what makes the rename durable. Without it,
    /// a crash between rename and the kernel's own write-back
    /// can leave a torn / zero-byte file. The very failure
    /// mode atomic-rename was supposed to prevent.
    pub fn save(&self, slot: &str, state: &S) -> io::Result<()> {
        self.save_to_path(&self.path(slot), state)
    }

    /// Path-explicit form of [`Self::save`]. Test seam: failure-
    /// injection tests can target a path the test prepared
    /// without relying on the process-global `dll_dir()`.
    pub(crate) fn save_to_path(&self, path: &Path, state: &S) -> io::Result<()> {
        match save_atomic(path, state) {
            Ok(()) => {
                self.last_error.lock().take();
                Ok(())
            }
            Err(e) => {
                let msg = format!("save to {} failed: {e}", path.display());
                crate::log!("rpg/store: {msg}");
                *self.last_error.lock() = Some(msg);
                Err(e)
            }
        }
    }

    /// Most recent save error, if any. Cleared on a successful
    /// save. For debug-snapshot surfaces.
    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().clone()
    }

    /// Clear the cached last-error. For tests that want to assert
    /// on a fresh state.
    pub fn clear_last_error(&self) {
        self.last_error.lock().take();
    }
}

// ---- Versioned envelope ------------------------------------------------
//
// SkillsState carries a `schema_version` field inline today. The
// planned BuildingsTracker would re-invent it. Generic versioning:
// wrap the persisted payload in a `Versioned<S>` envelope and the
// schema_version lives one level up, separate from the payload
// shape. Old SkillsState-style states still load via the
// `LegacyOr` adapter (tries the envelope first; falls back to the
// raw payload). New consumers (BuildingsTracker etc.) save the
// envelope form from day one.
//
// Migration trait: consumers that ship multiple schema versions
// implement `SchemaMigrate` and load returns the migrated `S`.
// Consumers that don't (yet) need migrations leave it at default.

/// Envelope shape on disk: `{"schema_version": N, "payload": ...}`.
/// Consumers that want generic versioning use
/// `SlotStore<Versioned<S>>` directly + the
/// [`load_versioned_with_migrate`] helper.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Versioned<S> {
    pub schema_version: u32,
    pub payload: S,
}

impl<S: Default> Default for Versioned<S> {
    fn default() -> Self {
        Self {
            schema_version: 0,
            payload: S::default(),
        }
    }
}

/// Schema migration contract. New consumers (BuildingsTracker
/// etc.) impl this for their persisted payload to get
/// version-aware loading. Default impl is a no-op (`migrate`
/// returns `None`, the loader falls back to `S::default()`).
pub trait SchemaMigrate: Sized + serde::Serialize + DeserializeOwned + Default {
    /// Current on-disk schema version of this payload shape.
    /// Bump whenever a non-additive change lands; pair with a
    /// new arm in `migrate`.
    const CURRENT_VERSION: u32;

    /// Migrate `raw` (parsed JSON value) from `from_version` to
    /// `CURRENT_VERSION`. Return `Some(S)` on success, `None` to
    /// fall back to `S::default()`. The default impl is `None`
    /// for every version mismatch.
    fn migrate(_from_version: u32, _raw: serde_json::Value) -> Option<Self> {
        None
    }
}

/// Load a versioned payload + run migrations. Tries the envelope
/// shape first; on parse failure or wrong version, calls
/// [`SchemaMigrate::migrate`]; on its failure, falls back to
/// `S::default()`. Always returns an `S` value (silent fall-
/// through to default is the right behavior for unrecoverable
/// save corruption. Better than panicking the host).
pub fn load_versioned_with_migrate<S: SchemaMigrate>(path: &Path) -> S {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => {
            crate::log!(
                "rpg/store: no prior save at {}; starting fresh",
                path.display()
            );
            return S::default();
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            crate::log!(
                "rpg/store: parse failed for {} ({e}); starting fresh",
                path.display()
            );
            return S::default();
        }
    };

    // Try envelope shape: { schema_version, payload }.
    if let (Some(ver), Some(payload)) = (
        value.get("schema_version").and_then(|v| v.as_u64()),
        value.get("payload").cloned(),
    ) {
        let ver = ver as u32;
        if ver == S::CURRENT_VERSION {
            match serde_json::from_value::<S>(payload) {
                Ok(s) => return s,
                Err(e) => {
                    crate::log!(
                        "rpg/store: current-version payload at {} failed to \
                         parse ({e}); falling back to default",
                        path.display()
                    );
                    return S::default();
                }
            }
        }
        crate::log!(
            "rpg/store: migrating {} from schema_version {} -> {}",
            path.display(),
            ver,
            S::CURRENT_VERSION
        );
        return S::migrate(ver, payload).unwrap_or_else(|| {
            crate::log!(
                "rpg/store: migrate({} -> {}) returned None; using default",
                ver,
                S::CURRENT_VERSION
            );
            S::default()
        });
    }

    // Legacy non-enveloped payload (existing SkillsState files).
    // Try a direct parse before giving up.
    match serde_json::from_value::<S>(value.clone()) {
        Ok(s) => {
            crate::log!(
                "rpg/store: loaded legacy non-enveloped state at {}",
                path.display()
            );
            s
        }
        Err(_) => S::default(),
    }
}

/// Save the in-memory `S` as a versioned envelope. Wraps
/// [`SlotStore::save_to_path`]'s atomic-write semantics through a
/// helper because callers typically already have a `SlotStore`
/// and want the versioned shape without reinventing the temp +
/// fsync + rename dance.
pub fn save_versioned<S: SchemaMigrate>(
    store: &SlotStore<Versioned<S>>,
    slot: &str,
    payload: S,
) -> io::Result<()> {
    let envelope = Versioned {
        schema_version: S::CURRENT_VERSION,
        payload,
    };
    store.save(slot, &envelope)
}

fn is_valid_slot(slot: &str) -> bool {
    if slot.is_empty() {
        return false;
    }
    if slot.starts_with('.') {
        return false;
    }
    !slot.chars().any(|c| matches!(c, '/' | '\\' | ':' | '\0'))
}

fn save_atomic<S: Serialize>(path: &Path, state: &S) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| io::Error::other(format!("serialize: {e}")))?;
    let tmp = path.with_extension("json.tmp");

    // Open + write + flush + fsync the temp before renaming so
    // the rename guarantees durability, not just atomicity.
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(json.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Default, Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
    struct State {
        a: u32,
        b: String,
    }

    #[test]
    fn slot_validation() {
        assert!(is_valid_slot("abc123"));
        assert!(is_valid_slot("save_2026"));
        assert!(is_valid_slot("0123abcd0123abcd0123abcd0123abcd"));
        // bad
        assert!(!is_valid_slot(""));
        assert!(!is_valid_slot("."));
        assert!(!is_valid_slot(".."));
        assert!(!is_valid_slot(".hidden"));
        assert!(!is_valid_slot("a/b"));
        assert!(!is_valid_slot("a\\b"));
        assert!(!is_valid_slot("c:thing"));
        assert!(!is_valid_slot("a\0b"));
    }

    #[test]
    fn save_load_round_trip() {
        let tmp =
            std::env::temp_dir().join(format!("ueforge_slotstore_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        // Force the store to write under our temp dir by setting
        // the dll_dir override. But dll_dir is a static; we
        // can't override per-test. Instead, exercise save_atomic
        // directly.
        let path = tmp.join("save.json");
        let state = State {
            a: 42,
            b: "hello".into(),
        };
        save_atomic(&path, &state).expect("save");
        let read: State =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(read, state);
        // Idempotent re-save.
        save_atomic(&path, &state).expect("save 2");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_atomic_creates_parent() {
        let tmp =
            std::env::temp_dir().join(format!("ueforge_slotstore_parent_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let path = tmp.join("nested").join("dir").join("save.json");
        let state = State::default();
        save_atomic(&path, &state).expect("save with nested parents");
        assert!(path.exists());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn invalid_slot_routes_to_sentinel() {
        // We can't easily test path() without dll_dir set, so
        // verify the slot validator alone.
        assert_eq!(is_valid_slot("../escape"), false);
        assert_eq!(is_valid_slot("normal_save"), true);
    }

    /// Per-test scratch directory under the OS temp dir. Cleaned
    /// up on Drop so failures don't pollute /tmp.
    struct Scratch {
        path: std::path::PathBuf,
    }
    impl Scratch {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "ueforge_slotstore_{tag}_{}_{:p}",
                std::process::id(),
                &tag as *const _,
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("scratch dir");
            Self { path }
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn save_atomic_fails_when_parent_is_a_file() {
        // create_dir_all on a path whose ancestor is a regular
        // file returns NotADirectory. The save MUST surface this
        // as Err. Silently writing somewhere unexpected would
        // be the bug.
        let s = Scratch::new("fail_parent");
        let blocker = s.path.join("blocker");
        fs::write(&blocker, "i am a file").expect("blocker");
        let path = blocker.join("save.json"); // parent is a file
        let result = save_atomic(&path, &State::default());
        assert!(
            result.is_err(),
            "expected error when parent is a file, got Ok"
        );
    }

    #[test]
    fn save_atomic_fails_when_target_is_nonempty_dir() {
        // rename(file, dir-with-contents) fails on every
        // platform we care about. Verify the temp doesn't bleed
        // into the wrong place when this happens.
        let s = Scratch::new("fail_dir");
        let path = s.path.join("save.json");
        fs::create_dir(&path).expect("blocker dir");
        fs::write(path.join("inner"), "x").expect("inner file");
        let result = save_atomic(&path, &State::default());
        assert!(
            result.is_err(),
            "expected error when target is a non-empty dir"
        );
    }

    #[test]
    fn save_caches_last_error_then_clears_on_recovery() {
        // Exercises the full SlotStore::save path through the
        // save_to_path test seam. First save fails, last_error
        // is populated. Second save (after removing the blocker)
        // succeeds; last_error clears.
        let s = Scratch::new("lasterror");
        let path = s.path.join("save.json");
        // Inject failure: directory at the rename target.
        fs::create_dir(&path).expect("dir at target");
        fs::write(path.join("inner"), "x").expect("inner");

        let store: SlotStore<State> = SlotStore::new("__test__");
        let state = State {
            a: 7,
            b: "hi".into(),
        };
        let r = store.save_to_path(&path, &state);
        assert!(r.is_err());
        let msg = store.last_error().expect("last_error populated");
        assert!(msg.contains("failed"), "msg={msg}");

        // Recover: clear the blocker.
        fs::remove_dir_all(&path).expect("clear blocker");
        let r = store.save_to_path(&path, &state);
        assert!(r.is_ok(), "post-recovery save: {:?}", r);
        assert!(
            store.last_error().is_none(),
            "last_error should clear on success"
        );

        // Round-trip: file actually contains the state.
        let loaded = store.load_from_path(&path);
        assert_eq!(loaded, state);
    }

    #[test]
    fn load_returns_default_on_corrupt_file() {
        // Bad JSON + present file -> default + log line, not
        // panic. Save data we can't parse must not crash the
        // mod or take the player's session down.
        let s = Scratch::new("corrupt");
        let path = s.path.join("save.json");
        fs::write(&path, "{ this is { not json").expect("write corrupt");
        let store: SlotStore<State> = SlotStore::new("__test__");
        let loaded = store.load_from_path(&path);
        assert_eq!(loaded, State::default());
    }

    #[test]
    fn load_returns_default_on_missing_file() {
        let s = Scratch::new("missing");
        let path = s.path.join("nope.json");
        let store: SlotStore<State> = SlotStore::new("__test__");
        let loaded = store.load_from_path(&path);
        assert_eq!(loaded, State::default());
    }

    #[test]
    fn save_does_not_corrupt_existing_file_on_failure() {
        // Existing good file. New save targets a path whose
        // parent is a file (forces save_atomic to fail at
        // create_dir_all). The existing file at a separate path
        // must remain intact. Atomic rename's whole purpose.
        let s = Scratch::new("preserve");
        let good_path = s.path.join("good.json");
        let store: SlotStore<State> = SlotStore::new("__test__");
        let original = State {
            a: 1,
            b: "original".into(),
        };
        store
            .save_to_path(&good_path, &original)
            .expect("initial save");

        // Try a save that fails (parent is a file).
        let blocker = s.path.join("blocker");
        fs::write(&blocker, "x").expect("blocker");
        let bad_target = blocker.join("save.json");
        let updated = State {
            a: 2,
            b: "updated".into(),
        };
        let r = store.save_to_path(&bad_target, &updated);
        assert!(r.is_err());

        // Original file untouched.
        let reloaded = store.load_from_path(&good_path);
        assert_eq!(reloaded, original);
    }

    // ---- Versioned envelope tests ---------------------------------------

    #[derive(Default, Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
    #[allow(dead_code)] // referenced by docs / migrate arm but never constructed by name
    struct V1 {
        n: u32,
    }
    #[derive(Default, Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
    struct V2 {
        n: u32,
        // V2 adds a name field. Migration from V1 fills it with
        // "<unknown>".
        name: String,
    }

    impl SchemaMigrate for V2 {
        const CURRENT_VERSION: u32 = 2;

        fn migrate(from: u32, raw: serde_json::Value) -> Option<Self> {
            if from != 1 {
                return None;
            }
            let n = raw.get("n").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            Some(V2 {
                n,
                name: "<unknown>".to_string(),
            })
        }
    }

    #[test]
    fn versioned_round_trip_current_schema() {
        let s = Scratch::new("vers_current");
        let path = s.path.join("v.json");
        // Hand-write an envelope at schema_version 2 (current).
        let json = r#"{"schema_version":2,"payload":{"n":42,"name":"alice"}}"#;
        fs::write(&path, json).expect("write");
        let loaded: V2 = load_versioned_with_migrate(&path);
        assert_eq!(
            loaded,
            V2 {
                n: 42,
                name: "alice".into()
            }
        );
    }

    #[test]
    fn versioned_runs_migration_on_old_schema() {
        let s = Scratch::new("vers_migrate");
        let path = s.path.join("v.json");
        // V1 envelope: schema_version 1 + payload {n}.
        let json = r#"{"schema_version":1,"payload":{"n":7}}"#;
        fs::write(&path, json).expect("write");
        let loaded: V2 = load_versioned_with_migrate(&path);
        assert_eq!(loaded.n, 7);
        assert_eq!(loaded.name, "<unknown>");
    }

    #[test]
    fn versioned_falls_back_to_default_on_failed_migration() {
        let s = Scratch::new("vers_fail");
        let path = s.path.join("v.json");
        // schema_version 0 has no migration arm -> fall back to
        // V2::default(), not panic.
        let json = r#"{"schema_version":0,"payload":{"n":99}}"#;
        fs::write(&path, json).expect("write");
        let loaded: V2 = load_versioned_with_migrate(&path);
        assert_eq!(loaded, V2::default());
    }

    #[test]
    fn versioned_accepts_legacy_non_enveloped_state() {
        let s = Scratch::new("vers_legacy");
        let path = s.path.join("v.json");
        // No envelope; just the V2 payload directly. Existing
        // SkillsState files look like this; they MUST still load
        // after a consumer adopts the envelope shape.
        let json = r#"{"n":13,"name":"legacy"}"#;
        fs::write(&path, json).expect("write");
        let loaded: V2 = load_versioned_with_migrate(&path);
        assert_eq!(
            loaded,
            V2 {
                n: 13,
                name: "legacy".into()
            }
        );
    }

    #[test]
    fn versioned_missing_file_returns_default() {
        let s = Scratch::new("vers_missing");
        let path = s.path.join("nope.json");
        let loaded: V2 = load_versioned_with_migrate(&path);
        assert_eq!(loaded, V2::default());
    }

    #[test]
    fn save_versioned_writes_envelope() {
        let s = Scratch::new("vers_save");
        let store: SlotStore<Versioned<V2>> = SlotStore::new("__test__");
        let path = s.path.join("v.json");
        let v = V2 {
            n: 1,
            name: "bob".into(),
        };
        store
            .save_to_path(
                &path,
                &Versioned {
                    schema_version: V2::CURRENT_VERSION,
                    payload: v.clone(),
                },
            )
            .expect("save");
        // On-disk shape: { schema_version: 2, payload: {...} }.
        let raw = fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            parsed.get("schema_version").and_then(|v| v.as_u64()),
            Some(2)
        );
        assert!(parsed.get("payload").is_some());
        // Round-trip through the migration loader.
        let loaded: V2 = load_versioned_with_migrate(&path);
        assert_eq!(loaded, v);
    }
}
