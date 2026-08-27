//! G2 binder for `ueforge::fall::FallHook`. PE-install plumbing
//! and OnLanded filtering live framework-side; this file owns:
//!
//! - G2-specific `FallHookConfig` (player BP classes, CMC offset,
//!   absolute Velocity.Z offset).
//! - G2FallBinder: drains the debug PE queue, applies the
//!   Fall Resistance velocity stomp, and fires
//!   `TriggerCtx::Fall` to subscribers via `TRACKER.fire`.
//! - The Grounded 2 layout and first-player selection for the
//!   debug status-effect snapshot.

use ueforge::fall::{FallBinder, FallHook, FallHookConfig};
use ueforge::hook::ProcessEventHook;
use ueforge::rpg::{FallEvent, TriggerCtx};
use ueforge::ue::TypedField;
use ueforge::ue::status_effect::{self, StatusEffectEntry, StatusEffectLayout};

use crate::rpg::{skills, tracker};

const G2_FALL_CONFIG: FallHookConfig = FallHookConfig {
    player_classes: &[
        "BP_SurvivalPlayerCharacter_C",
        "BP_SurvivalPlayerCharacter_Female02_C",
        "BP_SurvivalPlayerCharacter_Gellarde_C",
    ],
    on_landed_fn: "OnLanded",
    // ASurvivalCharacter.CharMovementComponent ptr at +0x1380.
    char_movement_component_offset: 0x1380,
    // UMovementComponent.Velocity (FVector, doubles) at +0xD8.
    // FVector.Z at +0x10 inside FVector -> absolute +0xE8 on CMC.
    velocity_z_offset: 0x00E8,
};

static HOOK: FallHook<G2FallBinder> = FallHook::new(G2_FALL_CONFIG);

const G2_STATUS_EFFECT_LAYOUT: StatusEffectLayout = StatusEffectLayout {
    component_offset: 0x1378,
    effects_array_offset: 0x01C8,
    row_handle_offset: 0x0058,
    type_offset: 0x30,
    value_offset: 0x34,
    max_effects: 64,
};

struct G2FallBinder;

impl FallBinder for G2FallBinder {
    /// Applies the purchased fall resistance before Grounded 2 calculates landing damage.
    /// Stays here because it writes Grounded 2's movement field and fires this mod's skill catalog;
    /// Ueforge owns landing detection and hook dispatch.
    fn before(&self, event: &FallEvent) {
        let _t = ueforge::counters::time_scope(&crate::counters::TIME_NS_FALL_HOOK);
        ueforge::counters::bump(&crate::counters::FALL_HOOK_FIRES);

        // Fall Resistance velocity stomp. The framework already
        // resolved `event.cmc` for us; we just compute the scale
        // and write Velocity.Z back. OnLanded fires before native
        // ApplyFallDamage reads live Velocity.Z, so this lands in
        // time.
        let reduction = current_fall_resistance_reduction();
        if reduction > 0.0
            && let Some(cmc) = event.cmc
        {
            let scale = (1.0 - reduction).max(0.0) as f64;
            let vz: TypedField<f64> = TypedField::at(G2_FALL_CONFIG.velocity_z_offset);
            let before = event.velocity_z_before;
            let after = before * scale;
            // SAFETY: `cmc` is the resolved CharMovementComponent
            // returned by ueforge::fall::FallHook for the player
            // pawn; writing f64 at the configured Velocity.Z
            // offset matches UMovementComponent's FVector layout
            // (UE5 doubles at CMC+0xE8). We are on the game
            // thread inside the OnLanded PE trampoline, before
            // native ApplyFallDamage reads the value.
            unsafe { vz.write(cmc, after) };
            if before.abs() > 0.001 {
                ueforge::log!(
                    "rpg/fall: stomped Velocity.Z {:.2} -> {:.2} on {} (reduction={:.3})",
                    before,
                    after,
                    event.player.name(),
                    reduction,
                );
            }
        }

        // Fire to subscribed Effects (none yet. 5c.4 follow-up).
        tracker::TRACKER.fire(&TriggerCtx::Engine(ueforge::rpg::UeEvent::Fall(event)));
    }
}

/// Installs landing hooks on every supported Grounded 2 player character class.
/// Stays here because the player classes and movement offsets are Grounded 2 facts;
/// Ueforge owns the reusable fall hook.
pub fn install() -> Result<Vec<ProcessEventHook>, &'static str> {
    HOOK.install(G2FallBinder)
}

/// Calculates the live fall-resistance reduction from the purchased Grounded 2 skill level.
/// Stays here because it reads this mod's catalog and tuning; Modforge owns progression math and state.
fn current_fall_resistance_reduction() -> f32 {
    tracker::with_state(|state| {
        let level = state
            .skill_levels
            .get(skills::SKILL_FALL_RESISTANCE)
            .copied()
            .unwrap_or(0);
        if level == 0 {
            return 0.0;
        }
        skills::skill_bonus(1.0, level).min(1.0)
    })
    .unwrap_or(0.0)
}

// ---------------------------------------------------------------
// Grounded 2 player selection for the debug status-effect snapshot.
// ---------------------------------------------------------------

/// Reports the status effects currently attached to the first live Grounded 2 player.
/// Stays here because Grounded 2 chooses the player and supplies its layout;
/// Ueforge owns the reusable Unreal traversal and row inspection.
pub fn snapshot_player_status_effects() -> Option<Vec<StatusEffectEntry>> {
    use crate::rpg::apply;

    let mut out: Option<Vec<StatusEffectEntry>> = None;
    apply::apply_to_live_player_characters(|player| {
        if out.is_none() {
            out = Some(status_effect::read_active(player, G2_STATUS_EFFECT_LAYOUT));
        }
    });
    out
}
