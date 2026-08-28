// RPG ImGui tab. Body lives in `ueforge::rpg::tab::render`; this
// crate supplies the static Tracker and the on/off toggle hooks
// (which route into g2rpg's `apply` module's process-global
// disabled-skills set).

use ueforge::rpg::tab::{ToggleFns, render as render_rpg_tab};

use crate::rpg::{apply, tracker};

/// Routes a skill toggle from the RPG tab into Grounded 2's active skill state.
/// Stays here because this adapter connects this mod's catalog to Ueforge's reusable tab.
fn set_enabled_void(skill_id: &'static str, enabled: bool) {
    apply::set_skill_enabled(skill_id, enabled);
}

const TOGGLES: ToggleFns = ToggleFns {
    is_enabled: apply::is_skill_enabled,
    set_enabled: set_enabled_void,
};

/// Draws Grounded 2's RPG skill controls and current progression.
/// Stays here because it supplies this mod's tracker and toggle policy;
/// Ueforge owns the reusable RPG tab renderer.
pub fn render() {
    let _t = ueforge::counters::time_scope(&crate::counters::TIME_NS_IMGUI_GET_XP);
    ueforge::counters::bump(&crate::counters::IMGUI_TAB_RENDERS);
    render_rpg_tab(&tracker::TRACKER, Some(&TOGGLES));
}
