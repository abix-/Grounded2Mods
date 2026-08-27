//! The shining ("emission") timer, exposed to the player.
//!
//! MISERY ends an expedition by regenerating the world when the
//! emission countdown reaches zero. Everything about that clock
//! lives on one Blueprint actor, `BP_GlobalManager_C`. See
//! `docs/research.md` sections 8, 14, 15, 17, 19.
//!
//! Two controls, both proven live before this module existed:
//! freeze the countdown, and set how many seconds are left.

use std::sync::atomic::{AtomicI32, Ordering};

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

/// Both found once a session; each `find_actor` is a full search
/// of the object list.
static MANAGER: ueforge::ue::actor::LiveActor =
    ueforge::ue::actor::LiveActor::new("BP_GlobalManager_C");
static DOOR: ueforge::ue::actor::LiveActor =
    ueforge::ue::actor::LiveActor::new("BP_ExpeditionDoor_C");
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
    /// Formats the time until the next Shining for a player to read quickly.
    /// Stays here because the wording is presentation for MISERY's Shining feature.
    pub fn pretty_remaining(&self) -> String {
        let total = self.seconds_left.max(0.0) as i64;
        format!("{}m {:02}s", total / 60, total % 60)
    }

    /// The four generators are the four areas. Numbers confirmed
    /// live: 2 = Meadows, 3 = Paneli (research doc 19). Factory
    /// and Bunker are 0 and 1 in an order nobody has pinned down.
    /// Turns MISERY's current area number into the location name shown to the player.
    /// Stays here because the area mapping is game content, not an engine concept.
    pub fn area_name(&self) -> &'static str {
        match self.area {
            2 => "Meadows",
            3 => "Paneli",
            0 | 1 => "Factory or Bunker (unmapped)",
            _ => "unknown",
        }
    }
}

/// Finds MISERY's live Shining manager, including the expedition-door fallback.
/// Stays here because the Blueprint classes and door offset are specific to this game.
fn manager_ptr() -> Result<*const u8, String> {
    if let Some(mgr) = MANAGER.ptr() {
        return Ok(mgr);
    }
    // Fallback: find the expedition door and follow its pointer
    // at +0x448 to the manager (research doc 20.4).
    let door = DOOR
        .ptr()
        .ok_or("no global manager or expedition door found")?;
    unsafe { follow_ptr_chain(door, &[DOOR_MGR_OFFSET]) }
}

/// Reads the next Shining countdown, state, area, and seed for display and controls.
/// Stays here because these fields use MISERY's manager layout; Ueforge supplies generic memory access.
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
/// Pauses or resumes the player's Shining countdown.
/// Stays here because the freeze field and feature behavior belong specifically to MISERY.
pub fn set_frozen(frozen: bool) -> Result<(), String> {
    let m = manager_ptr()?;
    unsafe { write_at(m, offset::FREEZE_TIMER, frozen as u8) };
    ueforge::log::log(format_args!("shining: frozen = {frozen}"));
    Ok(())
}

/// Set how many seconds remain before the next shining.
/// Sets exactly how long the player has until the next Shining.
/// Stays here because it writes MISERY's Shining countdown field.
pub fn set_seconds(seconds: f64) -> Result<(), String> {
    let m = manager_ptr()?;
    let clamped = seconds.max(0.0);
    unsafe { write_at(m, offset::TIME_UNTIL_EMMISION, clamped) };
    ueforge::log::log(format_args!("shining: {clamped}s until the next one"));
    Ok(())
}

/// Add seconds to whatever is left. What a player actually wants
/// mid-expedition: "give me ten more minutes".
/// Moves the next Shining earlier or later and returns the new countdown.
/// Stays here because it changes a MISERY event using this mod's player control policy.
pub fn add_seconds(seconds: f64) -> Result<f64, String> {
    let m = manager_ptr()?;
    let now: f64 = unsafe { read_at(m, offset::TIME_UNTIL_EMMISION) };
    let next = (now + seconds).max(0.0);
    unsafe { write_at(m, offset::TIME_UNTIL_EMMISION, next) };
    ueforge::log::log(format_args!("shining: {now} -> {next}s"));
    Ok(next)
}

// ---- UI ----

static SET_MINUTES: AtomicI32 = AtomicI32::new(20);

/// The tab redraws every frame; the manager is read once a
/// second. `modforge::ui::Cached` owns the timing.
static STATUS: modforge::ui::Cached<Result<Status, String>> = modforge::ui::Cached::new();
const REFRESH: std::time::Duration = std::time::Duration::from_secs(1);

/// Draws the Shining status and countdown controls for the player.
/// Stays here because it presents a MISERY-only event; Ueforge owns the reusable UI layer.
pub fn render() {
    use ueforge::ui;

    ui::text("Shining timer");
    ui::text_disabled(
        "The shining is what ends an expedition: the world outside \
         is regenerated, while your inventory and bunker carry over. \
         This tab stops that clock or moves it.",
    );
    ui::spacing();
    ui::separator();
    ui::spacing();

    let st = match STATUS.get(REFRESH, status) {
        Ok(st) => st,
        Err(e) => {
            ui::text_disabled("No expedition running.");
            ui::text_disabled(&format!("({e})"));
            if ui::button("Retry") {
                STATUS.invalidate();
            }
            return;
        }
    };

    ui::text(&format!("Next shining in   {}", st.pretty_remaining()));
    ui::text(&format!("Shinings so far   {}", st.shinings));
    ui::text(&format!("Area              {}", st.area_name()));
    ui::text_disabled(&format!("world seed {}", st.seed));
    ui::spacing();

    if ui::button("Refresh") {
        STATUS.invalidate();
    }

    ui::spacing();

    let mut frozen = st.frozen;
    if ui::checkbox("Pause the timer", &mut frozen) {
        if let Err(e) = set_frozen(frozen) {
            ueforge::log::log(format_args!("shining: freeze failed: {e}"));
        }
        STATUS.invalidate();
    }
    if st.frozen {
        ui::text_colored("Paused. The expedition will not end on its own.", (0.4, 0.9, 0.4, 1.0));
    }

    ui::spacing();
    ui::separator();
    ui::spacing();

    ui::text("Set the countdown");
    let mut minutes = SET_MINUTES.load(Ordering::Relaxed);
    ui::set_next_item_width(220.0);
    if ui::slider_i32("Minutes", &mut minutes, 1, 120) {
        SET_MINUTES.store(minutes, Ordering::Relaxed);
    }
    if ui::button("Set") {
        if let Err(e) = set_seconds(f64::from(minutes) * 60.0) {
            ueforge::log::log(format_args!("shining: set failed: {e}"));
        }
        STATUS.invalidate();
    }

    ui::same_line();
    if ui::button("10 sec") {
        if let Err(e) = set_seconds(10.0) {
            ueforge::log::log(format_args!("shining: set 10s failed: {e}"));
        }
        STATUS.invalidate();
    }

    ui::spacing();
    ui::text_disabled("Or add time to what is left");
    for (label, secs) in [("+5 min", 300.0), ("+10 min", 600.0), ("+30 min", 1800.0)] {
        if ui::button(label) {
            if let Err(e) = add_seconds(secs) {
                ueforge::log::log(format_args!("shining: add failed: {e}"));
            }
            STATUS.invalidate();
        }
        ui::same_line();
    }
    ui::new_line();

    ui::spacing();
    ui::separator();
    ui::text_disabled(
        "Pausing forever means the world outside never regenerates. \
         Whether anything you want depends on that (loot, new areas) \
         is not yet known.",
    );
}
