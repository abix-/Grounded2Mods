//! The shining ("emission") timer, exposed to the player.
//!
//! MISERY ends an expedition by regenerating the world when the
//! emission countdown reaches zero. Everything about that clock
//! lives on one Blueprint actor, `BP_GlobalManager_C`. See
//! `docs/misery-research.md` sections 8, 14, 15, 17, 19.
//!
//! Two controls, both proven live before this module existed:
//! freeze the countdown, and set how many seconds are left.

use ueforge::ue;
use ueforge::ue::{follow_ptr_chain, read_at, write_at};

/// Property offsets on `BP_GlobalManager_C`, from the UE4SS
/// object dump and confirmed by live reads (research doc 8.1).
/// Read by name at runtime instead if a game patch moves them.
mod offset {
    pub const EMISSIONS_COUNT: usize = 0x2A8;
    pub const TIME_UNTIL_EMMISION: usize = 0x2B0;
    pub const FREEZE_TIMER: usize = 0x2B8;
    pub const CURRENT_WORLD_SEED: usize = 0x2BC;
    pub const CURRENT_GENERATED_LEVEL: usize = 0x2C8;
}

const MGR_CLASS: &str = "BP_GlobalManager_C";
const DOOR_CLASS: &str = "BP_ExpeditionDoor_C";
const DOOR_MGR_OFFSET: usize = 0x448;

/// Everything the tab shows in one read, so the UI never holds a
/// stale half-state.
#[derive(Clone, Copy, Debug)]
pub struct Status {
    pub seconds_left: f64,
    pub shinings: i32,
    pub frozen: bool,
    pub area: u8,
    pub seed: i32,
}

impl Status {
    /// "12m 25s", the form a player reads at a glance.
    pub fn pretty_remaining(&self) -> String {
        let total = self.seconds_left.max(0.0) as i64;
        format!("{}m {:02}s", total / 60, total % 60)
    }

    /// The four generators are the four areas. Numbers confirmed
    /// live: 2 = Meadows, 3 = Paneli (research doc 19). Factory
    /// and Bunker are 0 and 1 in an order nobody has pinned down.
    pub fn area_name(&self) -> &'static str {
        match self.area {
            2 => "Meadows",
            3 => "Paneli",
            0 | 1 => "Factory or Bunker (unmapped)",
            _ => "unknown",
        }
    }
}

fn manager_ptr() -> Result<*const u8, String> {
    if let Some(mgr) = ue::actor::find_actor(MGR_CLASS, None) {
        return Ok(mgr);
    }
    // Fallback: find the expedition door and follow its pointer
    // at +0x448 to the manager (research doc 20.4).
    let door = ue::actor::find_actor(DOOR_CLASS, None)
        .ok_or("no global manager or expedition door found")?;
    unsafe { follow_ptr_chain(door, &[DOOR_MGR_OFFSET]) }
}

pub fn status() -> Result<Status, String> {
    let m = manager_ptr()?;
    Ok(Status {
        seconds_left: unsafe { read_at(m, offset::TIME_UNTIL_EMMISION) },
        shinings: unsafe { read_at(m, offset::EMISSIONS_COUNT) },
        frozen: unsafe { read_at::<u8>(m, offset::FREEZE_TIMER) } != 0,
        area: unsafe { read_at(m, offset::CURRENT_GENERATED_LEVEL) },
        seed: unsafe { read_at(m, offset::CURRENT_WORLD_SEED) },
    })
}

/// Stop or restart the countdown. This is the game's own flag,
/// not a mod invention: `BP_GlobalManager_C` has `FreezeTime` and
/// `UnfreezeTime` functions that drive the same bool.
pub fn set_frozen(frozen: bool) -> Result<(), String> {
    let m = manager_ptr()?;
    unsafe { write_at(m, offset::FREEZE_TIMER, frozen as u8) };
    ueforge::log::log(format_args!("shining: frozen = {frozen}"));
    Ok(())
}

/// Set how many seconds remain before the next shining.
pub fn set_seconds(seconds: f64) -> Result<(), String> {
    let m = manager_ptr()?;
    let clamped = seconds.max(0.0);
    unsafe { write_at(m, offset::TIME_UNTIL_EMMISION, clamped) };
    ueforge::log::log(format_args!("shining: {clamped}s until the next one"));
    Ok(())
}

/// Add seconds to whatever is left. What a player actually wants
/// mid-expedition: "give me ten more minutes".
pub fn add_seconds(seconds: f64) -> Result<f64, String> {
    let m = manager_ptr()?;
    let now: f64 = unsafe { read_at(m, offset::TIME_UNTIL_EMMISION) };
    let next = (now + seconds).max(0.0);
    unsafe { write_at(m, offset::TIME_UNTIL_EMMISION, next) };
    ueforge::log::log(format_args!("shining: {now} -> {next}s"));
    Ok(next)
}
