//! Actor / controller helpers shared across UE5 mods.
//!
//! These wrap UE5's universal class-chain layout and the
//! `AController.Pawn` slot at a stable offset that doesn't move
//! between Engine versions.

use crate::ue::{self, UClass, UObject};
use std::time::Duration;

/// `AController.Pawn` byte offset (Engine_classes.hpp:30510).
/// Stable UE5 layout, valid for any game built on Engine 5.x.
pub const A_CONTROLLER_PAWN_OFFSET: usize = 0x0308;

/// Walk `obj`'s class chain (its UClass + all ancestors via
/// `super_class()`) and return `true` if any class name contains
/// `needle`. Bounded depth (32) so a corrupted super-chain can't
/// loop forever.
///
/// ```ignore
/// if class_chain_contains(controller, "PlayerController") {
///     // ...
/// }
/// ```
pub fn class_chain_contains(obj: &UObject, needle: &str) -> bool {
    let Some(cls) = obj.class() else { return false };
    let mut cur: Option<&UClass> = Some(cls);
    let mut depth = 0;
    while let Some(c) = cur {
        if depth > 32 {
            return false;
        }
        if c.as_object().name().contains(needle) {
            return true;
        }
        cur = c.super_class();
        depth += 1;
    }
    false
}

/// Read the `AController.Pawn` slot from a controller. Returns
/// `None` if the controller has no possessed pawn.
pub fn controller_pawn(controller: &UObject) -> Option<&UObject> {
    unsafe {
        let p: *mut UObject = controller
            .field_ptr(A_CONTROLLER_PAWN_OFFSET)
            .cast::<*mut UObject>()
            .read_unaligned();
        p.as_ref()
    }
}

/// `true` if `this`'s outer's full name contains `needle`.
/// Common pattern: filter PE-hook fires by component owner
/// (`is_outer_named(hc, "BP_SurvivalPlayerCharacter")`).
pub fn is_outer_named(this: &UObject, needle: &str) -> bool {
    this.outer()
        .map(|o| o.full_name().contains(needle))
        .unwrap_or(false)
}

/// Class name of `this`'s outer, if any.
pub fn outer_class_name(this: &UObject) -> Option<String> {
    this.outer()
        .and_then(|o| o.class())
        .map(|c| c.as_object().name())
}

/// `"<name>(<class-name>)"` describing an object for log lines.
/// `None` becomes `"<none>"`.
pub fn describe(obj: Option<&UObject>) -> String {
    match obj {
        None => "<none>".to_string(),
        Some(o) => {
            let cls = o.class().map(|c| c.as_object().name()).unwrap_or_default();
            format!("{}({})", o.name(), cls)
        }
    }
}

/// Find a non-CDO instance whose class name matches
/// `class_name` and whose name contains `name_filter` (if
/// provided). Only returns objects in a PersistentLevel
/// (live world actors, not editor or CDO copies).
pub fn find_actor(class_name: &str, name_filter: Option<&str>) -> Option<*const u8> {
    find_object(class_name, name_filter, true)
}

/// Find a non-CDO instance by class name. When
/// `require_level` is false, matches any non-CDO instance
/// (useful for widgets and other non-actor objects).
pub fn find_object(
    class_name: &str,
    name_filter: Option<&str>,
    require_level: bool,
) -> Option<*const u8> {
    let rt = ue::try_runtime()?;
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return None;
    }
    for obj in view.iter() {
        if obj.is_default_object() {
            continue;
        }
        let class = obj.class()?;
        if class.as_object().name() != class_name {
            continue;
        }
        if let Some(filter) = name_filter {
            if !obj.name().contains(filter) {
                continue;
            }
        }
        if require_level && !obj.full_name().contains("PersistentLevel") {
            continue;
        }
        return Some(obj.as_ptr());
    }
    None
}

/// Find all live world actors whose class chain (own class or
/// any ancestor) contains `class_needle`. Unlike `find_actor`,
/// this matches subclasses, so a Blueprint base class like
/// `BP_MasterVendorBuildPart_C` finds every derived vendor.
/// Skips CDOs and objects outside a PersistentLevel.
pub fn find_actors_by_chain(class_needle: &str) -> Vec<*const u8> {
    let mut found = Vec::new();
    let Some(rt) = ue::try_runtime() else {
        return found;
    };
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return found;
    }
    for obj in view.iter() {
        if obj.is_default_object() {
            continue;
        }
        if !class_chain_contains(obj, class_needle) {
            continue;
        }
        if !obj.full_name().contains("PersistentLevel") {
            continue;
        }
        found.push(obj.as_ptr());
    }
    found
}

/// Spawn a background thread that calls `on_load` each time a
/// finder function returns `Some`. The finder is polled every
/// `poll_interval`. After `on_load` runs, the thread watches
/// for the finder to return `None` (player returned to main
/// menu), then re-polls and re-applies on the next load.
///
/// The thread runs for the lifetime of the process.
pub fn on_each_load<P, F>(
    label: &'static str,
    poll_interval: Duration,
    finder: P,
    on_load: F,
) where
    P: Fn() -> Option<*const u8> + Send + 'static,
    F: Fn(*const u8) + Send + 'static,
{
    let thread_name = format!("ueforge-load-{label}");
    let _ = std::thread::Builder::new().name(thread_name).spawn(move || {
        loop {
            std::thread::sleep(poll_interval);
            let Some(ptr) = finder() else {
                continue;
            };
            crate::log::log(format_args!("{label}: found, applying"));
            on_load(ptr);

            loop {
                std::thread::sleep(poll_interval);
                if finder().is_none() {
                    crate::log::log(format_args!(
                        "{label}: gone (main menu?), waiting for reload"
                    ));
                    break;
                }
            }
        }
    });
}
