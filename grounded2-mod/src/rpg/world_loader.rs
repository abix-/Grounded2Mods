// Eager state load on world entry.
//
// 1Hz poller (via ueforge::rpg::SlotPoller) that watches the G2 save
// slot resolver and drives tracker activate / deactivate transitions
// the moment the player enters or leaves the world.

use std::sync::OnceLock;
use std::time::Duration;

use ueforge::rpg::{PollerHandle, SlotPoller};

use crate::rpg::{save_slot, tracker};
use crate::settings::Settings;

const POLL_INTERVAL: Duration = Duration::from_secs(1);

static SETTINGS: OnceLock<Settings> = OnceLock::new();
static POLLER: OnceLock<PollerHandle> = OnceLock::new();

/// Starts watching for entry into or departure from a Grounded 2 playthrough.
/// Stays here because it binds Grounded 2's slot resolver and settings to Modforge's reusable poller.
pub fn spawn(settings: Settings) {
    let _ = SETTINGS.set(settings);
    let handle = SlotPoller::spawn(
        POLL_INTERVAL,
        || {
            ueforge::counters::bump(&crate::counters::WORLD_LOADER_POLLS);
            save_slot::current_slot_key()
        },
        |slot| tracker::activate_slot(slot, settings_clone()),
        || tracker::deactivate_slot(),
    );
    let _ = POLLER.set(handle);
}

/// Signal the poller to exit. Called from the mod's `on_shutdown`
/// hook so the worker thread doesn't outlive the unloaded DLL.
/// Bounded by `POLL_INTERVAL` (~1s).
/// Stays here because this function participates in Grounded 2's mod shutdown sequence.
pub fn shutdown() {
    if let Some(p) = POLLER.get() {
        p.stop();
    }
}

/// Most recent panic from the poller's resolver / activate /
/// deactivate callbacks. Snapshot surface; cleared on no-op tick.
/// Stays here because it exposes this mod's world watcher through the Grounded 2 debug snapshot.
pub fn last_panic() -> Option<String> {
    POLLER.get().and_then(|p| p.last_panic())
}

/// Total panics caught from the poller since spawn.
/// Stays here because it exposes this mod's world watcher through the Grounded 2 debug snapshot.
pub fn panic_count() -> u64 {
    POLLER.get().map(|p| p.panic_count()).unwrap_or(0)
}

/// Snapshot of settings loaded at init. Used by the debug endpoint.
/// Stays here because these are Grounded 2's settings; Modforge owns their generic file storage.
pub fn loaded_settings() -> Option<Settings> {
    SETTINGS.get().cloned()
}

/// Copies the active Grounded 2 settings for a newly detected playthrough.
/// Stays here because this adapter feeds this mod's settings into its tracker activation.
fn settings_clone() -> Settings {
    SETTINGS.get().cloned().unwrap_or_default()
}
