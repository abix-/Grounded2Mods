//! Shared ImGui widget for the `ueforge::dynamic_tweaks` framework.
//! Renders an inline section. Intended to be called from a game
//! crate's existing tab render function, not as a top-level tab.
//!
//! ```ignore
//! fn render_tweaks_tab() {
//!     // ... game-specific sections ...
//!     ueforge::ui::separator();
//!     ueforge::ui_dynamic_tweaks::render(
//!         &settings::get().get().dynamic_tweaks,
//!         || { let _ = settings::get().reload(); },
//!     );
//! }
//! ```
//!
//! The closure `reload_settings` lets the widget refresh the
//! consumer's settings before Re-apply. The game crate decides
//! where the settings live, so the widget can't reload them
//! itself. Pass an empty closure if reload-on-button isn't wanted.

use crate::dynamic_tweaks::DynamicTweak;
use crate::ui;

/// Render the dynamic-tweaks section. `tweaks` is the source-of-truth
/// slice; `reload_settings` runs on the "Re-apply all" button to
/// pick up settings.json edits before re-applying.
pub fn render<F: Fn()>(tweaks: &[DynamicTweak], reload_settings: F) {
    ui::text("Dynamic tweaks (settings.json)");
    ui::text_disabled(
        "Each entry is applied via name lookup against the discovery \
         cache. Edit settings.json + click Re-apply to take effect. \
         Vanilla rows are captured on first apply so changing the \
         multiplier never compounds.",
    );
    ui::spacing();

    if ui::button("Re-apply all##dyn_tweaks") {
        reload_settings();
        let _ = crate::dynamic_tweaks::apply_all(tweaks);
    }
    ui::same_line();
    if ui::button("Revert all##dyn_tweaks") {
        let _ = crate::dynamic_tweaks::revert_all();
    }
    ui::same_line();
    if ui::button("Refresh discovery##dyn_tweaks") {
        let _ = crate::discovery::refresh();
    }

    ui::spacing();
    let status = crate::dynamic_tweaks::last_status();
    if status.is_empty() {
        if tweaks.is_empty() {
            ui::text_disabled(
                "No tweaks declared. Add entries to `dynamic_tweaks` \
                 in settings.json.",
            );
        } else {
            ui::text_disabled("Tweaks declared but not yet applied. Click Re-apply all.");
        }
        return;
    }
    for st in &status {
        let head = format!(
            "{}  ({}.{}, {}={})",
            st.id, st.table, st.field, st.kind, st.value
        );
        ui::text(&head);
        match &st.last_result {
            Ok(n) => ui::text_disabled(&format!("  applied: {n} rows")),
            Err(e) => ui::text_colored(&format!("  error: {e}"), (1.0, 0.4, 0.4, 1.0)),
        }
    }
    ui::text_disabled(&format!(
        "apply count: {}",
        crate::dynamic_tweaks::apply_count()
    ));
}
