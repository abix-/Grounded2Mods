//! Memory scanner. Moved into `modforge::scanner` during Phase
//! 0b. ueforge installs the UE-flavored selector resolver
//! (UObject + selector grammar) here at init time via
//! `modforge::scanner::set_selector_resolver`.

pub use modforge::scanner::*;

/// Install the ueforge-side selector resolver into the modforge
/// scanner. Call once at worker init (before any `freeze` op
/// can be dispatched). Subsequent calls are no-ops.
pub fn install_ue_resolver() {
    modforge::scanner::set_selector_resolver(|selector, offset| {
        let obj = crate::selector::resolve(selector).map_err(|e| format!("freeze: {e}"))?;
        Ok((obj as *const crate::ue::UObject as usize).wrapping_add(offset))
    });
}
