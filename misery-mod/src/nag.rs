//! Hide the playtest notice without touching the keyboard.
//!
//! The old approach synthesised a spacebar press, which is a
//! hack and needs the game focused. This hides the widget
//! properly instead.
//!
//! The awkward part is thread affinity: UMG widgets may only be
//! touched from the game thread, and the notice appears at the
//! MAIN MENU, where this mod's usual drain site (the player
//! character) does not exist. So the widget's own class is
//! hooked: when the engine calls anything on the notice, we are
//! on the game thread with the notice in hand, and can dismiss
//! it there.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use ueforge::ue::{self, UObject};

/// The playtest notice. Absent from the object dump (it was not
/// loaded when that was written) but live at startup.
const NAG_CLASS: &str = "WD_PlaytestNote01_C";

/// The notice's own spacebar handler, read live off its class:
///
/// ```text
/// Get_KeyIcon_1_Brush
/// InpActEvt_SpaceBar_K2Node_InputKeyEvent_1
/// InpActEvt_Gamepad_FaceButton_Bottom_K2Node_InputKeyEvent_0
/// ExecuteUbergraph_WD_PlaytestNote01
/// ```
///
/// Calling it runs exactly what a real spacebar press runs, so
/// the game does its own teardown. Nothing is hidden and
/// nothing is destroyed.
const DISMISS_FN: &str = "InpActEvt_SpaceBar_K2Node_InputKeyEvent_1";

/// Guard against re-entry: pressing the handler goes through
/// ProcessEvent, so without this the observer would call itself.
static HIDING: AtomicBool = AtomicBool::new(false);
static HIDDEN_COUNT: AtomicU64 = AtomicU64::new(0);

/// Set once the hook is in, so the watcher stops trying.
static HOOKED: AtomicBool = AtomicBool::new(false);

pub fn install() {
    register_ops();
    std::mem::forget(modforge::rpg::poller::spawn_interval(
        "misery-nag",
        std::time::Duration::from_millis(500),
        // Looks for a live widget, so it runs on the game thread.
        ueforge::game_thread::each_tick(watch),
    ));
}

/// Wait for the notice to exist, then hook its class.
fn watch() {
    if HOOKED.load(Ordering::Acquire) {
        return;
    }
    if ue::try_runtime().is_none() {
        return;
    }
    // Widgets are not actors, so do not require a level.
    let Some(ptr) = ue::actor::find_object(NAG_CLASS, None, false) else {
        return;
    };
    // SAFETY: ptr came from this call's GObjects walk.
    let obj = unsafe { &*(ptr as *const UObject) };
    match ueforge::hook::ProcessEventHook::install_for_object(NAG_CLASS, obj, on_nag_event) {
        Ok(h) => {
            HOOKED.store(true, Ordering::Release);
            ueforge::log::log(format_args!("nag: hooked {NAG_CLASS}"));
            ueforge::hook::register(h);
        }
        Err(e) => {
            ueforge::log::log(format_args!("nag: hook failed ({e})"));
        }
    }
}

/// Game thread, with the notice as `this`. Dismiss it once.
///
/// CRITICAL: Blueprint widget classes add no C++ virtuals, so
/// they SHARE the base UUserWidget vtable. Patching "the
/// notice's vtable" therefore patches EVERY widget, and this
/// handler is called for all of them. Without this check the
/// main menu gets collapsed too, which it was.
fn on_nag_event(
    this: &UObject,
    function: &ueforge::ue::UFunction,
    parms: *mut c_void,
    original: ueforge::hook::OriginalProcessEvent,
) {
    let is_nag = this
        .class()
        .map(|c| c.as_object().name() == NAG_CLASS)
        .unwrap_or(false);
    // Let the engine finish its own call before touching the
    // widget.
    // SAFETY: engine-supplied arguments forwarded unchanged.
    unsafe { original.call(this, function, parms) };

    if is_nag && !HIDING.swap(true, Ordering::AcqRel) {
        dismiss(this);
        HIDING.store(false, Ordering::Release);
    }
}

/// Dismiss the notice for real. Game thread only.
///
/// Press the notice's own spacebar handler. Hiding the widget
/// was never dismissal: `SetVisibility(Collapsed)` left it
/// instantiated and still swallowing input, which is exactly
/// what the black screen was. `RemoveFromParent` is the other
/// wrong answer: it destroys an object whose vtable we have
/// patched, and the next virtual call lands in a half-destroyed
/// object ("Pure virtual function being called"). Crashed the
/// game 2026-08-26.
///
/// Running the game's own handler avoids both. The game tears
/// its own notice down the way it always does.
fn dismiss(widget: &UObject) {
    let Some(cls) = widget.class() else { return };
    let Some(f) = cls.get_function(NAG_CLASS, DISMISS_FN) else {
        ueforge::log::log(format_args!("nag: {NAG_CLASS} has no {DISMISS_FN}"));
        return;
    };
    // The handler is a K2Node input event, so it may declare an
    // FKey parameter. Size the block from the UFunction rather
    // than assuming: undersizing lets the callee write past the
    // buffer.
    let mut parms = vec![0u8; f.parms_size().max(1) as usize];
    // SAFETY: live widget on the game thread, function looked up
    // from that widget's own class, parm block sized from the
    // function itself and zeroed.
    unsafe {
        widget.process_event(f, parms.as_mut_ptr() as *mut c_void);
    }
    let n = HIDDEN_COUNT.fetch_add(1, Ordering::Relaxed);
    if n == 0 {
        ueforge::log::log(format_args!(
            "nag: pressed {DISMISS_FN} ({} parm bytes, {} parms)",
            f.parms_size(),
            f.num_parms()
        ));
    }
}

/// Every function the notice's own class defines. The proper
/// dismissal is one of these: the game's own teardown, rather
/// than us ripping the widget out. Read live because this class
/// is absent from the object dump.
pub fn nag_functions() -> Vec<String> {
    let mut out = Vec::new();
    let Some(ptr) = ue::actor::find_object(NAG_CLASS, None, false) else {
        return out;
    };
    // SAFETY: ptr came from this call's GObjects walk.
    let obj = unsafe { &*(ptr as *const UObject) };
    let Some(cls) = obj.class() else { return out };
    for (name, _flags) in cls.iter_functions() {
        out.push(name);
    }
    out
}

fn register_ops() {
    ueforge::ops::OP_REGISTRY.register(ueforge::ops::OpDef::new(
        "nag_stats",
        "Playtest notice suppression state",
        "{}",
        |_a| {
            Ok(serde_json::json!({
                "dismissals": HIDDEN_COUNT.load(Ordering::Relaxed),
                "hide_calls": HIDDEN_COUNT.load(Ordering::Relaxed),
                "present": ue::actor::find_object(NAG_CLASS, None, false).is_some(),
                "functions": nag_functions(),
            }))
        },
    ));
}
