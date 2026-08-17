//! The Speed tab: read and override movement speeds.
//!
//! Re-reads the live TMap every frame so the tab survives
//! main menu -> load transitions.

use ueforge::ui;

use crate::speed;

pub fn render() {
    ui::text("Movement speed");
    ui::text_disabled(
        "Multiplier applied to all movement states \
         (walk, sprint, crouch, holding weapon, etc).",
    );
    ui::spacing();
    ui::separator();
    ui::spacing();

    match speed::current_all() {
        Ok(entries) => {
            for e in &entries {
                ui::text(&format!("  key {:2}  {:.0}", e.key, e.speed));
            }
        }
        Err(e) => {
            ui::text_disabled("No player loaded.");
            ui::text_disabled(&format!("({e})"));
            return;
        }
    }

    ui::spacing();
    ui::separator();
    ui::spacing();
    ui::text("Quick set");
    for (label, mult) in [("1x (default)", 1.0), ("2x", 2.0), ("3x", 3.0)] {
        if ui::button(label) {
            if let Err(e) = speed::set_multiplier(mult) {
                ueforge::log::log(format_args!("speed: {label} failed: {e}"));
            }
        }
        ui::same_line();
    }
    ui::new_line();
}
