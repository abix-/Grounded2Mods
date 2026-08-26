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
//! on the game thread with the notice in hand, and can collapse
//! it there.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use ueforge::ue::{self, UObject};

/// The playtest notice. Absent from the object dump (it was not
/// loaded when that was written) but live at startup.
const NAG_CLASS: &str = "WD_PlaytestNote01_C";

/// ESlateVisibility::Collapsed: hidden and taking no layout
/// space. Preferred over RemoveFromParent, which destroys the
/// widget while the engine is mid-call on it.
const COLLAPSED: u8 = 1;

/// Set once the hook is in, so the watcher stops trying.
static HOOKED: AtomicBool = AtomicBool::new(false);
/// Guard against re-entry: SetVisibility itself goes through
/// ProcessEvent, so without this the handler would call itself.
static HIDING: AtomicBool = AtomicBool::new(false);
static HIDDEN_COUNT: AtomicU64 = AtomicU64::new(0);

pub fn install() {
    register_ops();
    std::mem::forget(modforge::rpg::poller::spawn_interval(
        "misery-nag",
        Duration::from_millis(500),
        watch,
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

/// Game thread, with the notice as `this`. Collapse it once.
fn on_nag_event(
    this: &UObject,
    function: &ueforge::ue::UFunction,
    parms: *mut c_void,
    original: ueforge::hook::OriginalProcessEvent,
) {
    // CRITICAL: Blueprint widget classes add no C++ virtuals, so
    // they SHARE the base UUserWidget vtable. Patching "the
    // notice's vtable" therefore patches EVERY widget, and this
    // handler is called for all of them. Without this check the
    // main menu gets collapsed too, which it was.
    let is_nag = this
        .class()
        .map(|c| c.as_object().name() == NAG_CLASS)
        .unwrap_or(false);
    // Let the engine finish its own call before the widget is
    // taken out from under it.
    // SAFETY: engine-supplied arguments forwarded unchanged.
    unsafe { original.call(this, function, parms) };

    if is_nag && !HIDING.swap(true, Ordering::AcqRel) {
        dismiss(this);
        HIDING.store(false, Ordering::Release);
    }
}

/// Dismiss the notice for real. Game thread only.
///
/// Collapsing it is not enough: the widget stays instantiated
/// and keeps swallowing input, which is what a black screen
/// waiting for a keypress actually is. Live reading at that
/// moment showed the notice still present with a loading circle
/// inside it, so this widget IS the blocking screen. Removing it
/// from its parent is what dismissing it means.
fn dismiss(widget: &UObject) {
    let Some(cls) = ue::find_class_fast("Widget") else { return };
    // Hide first so nothing flashes, then remove so it stops
    // taking input.
    if let Some(set_vis) = cls.get_function("Widget", "SetVisibility") {
        let mut parms = [COLLAPSED];
        // SAFETY: live widget on the game thread; SetVisibility
        // takes one byte.
        unsafe {
            widget.process_event(set_vis, parms.as_mut_ptr() as *mut c_void);
        }
    }
    // DO NOT call RemoveFromParent here. It destroys the widget
    // while our hook is still patched into its vtable, and the
    // next virtual call lands in a half-destroyed object:
    // "Pure virtual function being called", with a stack full of
    // recursion. Crashed the game 2026-08-26.
    //
    // Making it non-interactive is safe: the widget survives, so
    // nothing is destroyed under the engine's feet.
    if let Some(set_enabled) = cls.get_function("Widget", "SetIsEnabled") {
        let mut parms = [0u8];
        // SAFETY: live widget on the game thread; SetIsEnabled
        // takes one bool.
        unsafe {
            widget.process_event(set_enabled, parms.as_mut_ptr() as *mut c_void);
        }
    }
    let n = HIDDEN_COUNT.fetch_add(1, Ordering::Relaxed);
    if n == 0 {
        ueforge::log::log(format_args!(
            "nag: collapsed and disabled the playtest notice"
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
                "hooked": HOOKED.load(Ordering::Relaxed),
                "hide_calls": HIDDEN_COUNT.load(Ordering::Relaxed),
                "present": ue::actor::find_object(NAG_CLASS, None, false).is_some(),
                "functions": nag_functions(),
            }))
        },
    ));
}
