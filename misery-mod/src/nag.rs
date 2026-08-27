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

/// Has the notice been dismissed? Once it has, this feature is
/// finished for the session.
fn is_dismissed() -> bool {
    HIDDEN_COUNT.load(Ordering::Relaxed) > 0
}

/// Starts watching for MISERY's playtest notice and exposes its status.
/// Stays here because suppressing this particular game widget is a MISERY feature built on Ueforge hooks.
pub fn install() {
    register_ops();
    ueforge::hook::install_for_live_object_until(
        "misery-nag",
        std::time::Duration::from_millis(500),
        NAG_CLASS,
        || ue::actor::find_live_object(NAG_CLASS, None, false),
        on_nag_event,
        |result| match result {
            Ok(()) => ueforge::log::log(format_args!("nag: hooked {NAG_CLASS}")),
            Err(e) => ueforge::log::log(format_args!("nag: hook failed ({e})")),
        },
        // The notice is shown once per launch. Once it has been
        // dismissed there is nothing left to watch for, so the
        // hook comes out and the watcher ends itself. Leaving
        // them in cost 1326 ms of game thread in 30 seconds for
        // no work at all (docs/performance.md).
        is_dismissed,
    );
}

/// Game thread, with the notice as `this`. Dismiss it once.
///
/// CRITICAL: Blueprint widget classes add no C++ virtuals, so
/// they SHARE the base UUserWidget vtable. Patching "the
/// notice's vtable" therefore patches EVERY widget, and this
/// handler is called for all of them. Without this check the
/// main menu gets collapsed too, which it was.
/// Stays here because it filters Ueforge's shared hook for MISERY's exact notice class.
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

    // Once, and once only. The hook is taken out within half a
    // second of the first dismissal, and this closes the window
    // in between: a widget shares its vtable with every other
    // widget, so this handler is busy in that window.
    if is_nag && !is_dismissed() && !HIDING.swap(true, Ordering::AcqRel) {
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
/// Stays here because the dismissal function and widget lifecycle belong to MISERY's menu.
fn dismiss(widget: &UObject) {
    if widget.class().is_none() {
        return;
    }
    // SAFETY: this runs inside the notice's ProcessEvent hook on
    // the game thread, and this input event accepts zeroed defaults.
    let f = match unsafe { ue::pe_call::call_ufunction_zeroed(widget, NAG_CLASS, DISMISS_FN) } {
        Ok(function) => function,
        Err(e) => {
            ueforge::log::log(format_args!("nag: {e}"));
            return;
        }
    };
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
/// Stays here because this diagnostic inspects MISERY's notice class, not Unreal widgets generally.
pub fn nag_functions() -> Vec<String> {
    let mut out = Vec::new();
    let Some(obj) = ue::actor::find_live_object(NAG_CLASS, None, false) else {
        return out;
    };
    let Some(cls) = obj.class() else { return out };
    for (name, _flags) in cls.iter_functions() {
        out.push(name);
    }
    out
}

/// Adds the playtest-notice status command to the MISERY debug API.
/// Stays here because the reported state belongs solely to this mod's notice suppression.
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
