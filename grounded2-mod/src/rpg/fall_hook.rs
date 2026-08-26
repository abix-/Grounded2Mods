//! G2 binder for `ueforge::fall::FallHook`. PE-install plumbing
//! and OnLanded filtering live framework-side; this file owns:
//!
//! - G2-specific `FallHookConfig` (player BP classes, CMC offset,
//!   absolute Velocity.Z offset).
//! - G2FallBinder: drains the debug PE queue, applies the
//!   Fall Resistance velocity stomp, and fires
//!   `TriggerCtx::Fall` to subscribers via `TRACKER.fire`.
//! - Unrelated helpers that have historically lived in this file:
//!   `snapshot_player_status_effects`, `lookup_data_table_row`.

use ueforge::fall::{FallBinder, FallHook, FallHookConfig};
use ueforge::hook::ProcessEventHook;
use ueforge::rpg::{FallEvent, TriggerCtx};
use ueforge::ue::TypedField;
use ueforge::ue::UObject;

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

struct G2FallBinder;

impl FallBinder for G2FallBinder {
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

pub fn install() -> Result<Vec<ProcessEventHook>, &'static str> {
    HOOK.install(G2FallBinder)
}

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
// Unrelated helpers: player status-effect snapshot (used by the
// debug HTTP endpoint). Kept here for historical reasons; not
// part of the fall hook.
// ---------------------------------------------------------------

/// Look up a row in a `UDataTable` by FName-as-u64. Thin wrapper
/// over `ueforge::ue::tmap::find_value_by_fname_key` at the
/// `UDataTable.RowMap` offset.
pub(crate) fn lookup_data_table_row(table: &UObject, row_name: u64) -> Option<*const u8> {
    use ueforge::ue::offsets::datatable;
    unsafe { ueforge::ue::tmap::find_value_by_fname_key(table, datatable::ROW_MAP, row_name) }
}

/// Read `FStatusEffectData.Type` (u8 at +0x30) and `Value`
/// (f32 at +0x34) from the row bytes.
fn read_status_effect_row(row: *const u8) -> (u8, f32) {
    const TYPE_OFFSET: usize = 0x30;
    const VALUE_OFFSET: usize = 0x34;
    unsafe {
        let stat_type = (row.add(TYPE_OFFSET) as *const u8).read_unaligned();
        let value = (row.add(VALUE_OFFSET) as *const f32).read_unaligned();
        (stat_type, value)
    }
}

#[derive(Debug, Clone)]
pub struct StatusEffectEntry {
    pub row: String,
    pub table: String,
    pub stat_type: Option<u8>,
    pub value: Option<f32>,
}

pub fn snapshot_player_status_effects() -> Option<Vec<StatusEffectEntry>> {
    use crate::rpg::apply;

    let mut out: Option<Vec<StatusEffectEntry>> = None;
    apply::apply_to_live_player_characters(|player| {
        if out.is_none() {
            out = Some(collect_status_effects(player));
        }
    });
    out
}

fn collect_status_effects(player: &UObject) -> Vec<StatusEffectEntry> {
    const ASC_STATUS_EFFECT_COMPONENT: usize = 0x1378;
    const SEC_STATUS_EFFECTS_TARRAY: usize = 0x01C8;
    const STATUS_EFFECT_ROW_HANDLE: usize = 0x0058;

    let mut entries = Vec::new();
    let sec = unsafe {
        let p: *mut UObject = player
            .field_ptr(ASC_STATUS_EFFECT_COMPONENT)
            .cast::<*mut UObject>()
            .read_unaligned();
        match p.as_ref() {
            Some(c) => c,
            None => return entries,
        }
    };
    let (data_ptr, num) = unsafe {
        let base = sec.field_ptr(SEC_STATUS_EFFECTS_TARRAY);
        let data = (base as *const *const *mut UObject).read_unaligned();
        let num = (base.add(8) as *const i32).read_unaligned();
        (data, num.max(0) as usize)
    };
    if data_ptr.is_null() || num == 0 {
        return entries;
    }
    for i in 0..num.min(64) {
        let effect = unsafe {
            let p = data_ptr.add(i).read_unaligned();
            match p.as_ref() {
                Some(e) => e,
                None => continue,
            }
        };
        let table_ptr = unsafe {
            effect
                .field_ptr(STATUS_EFFECT_ROW_HANDLE)
                .cast::<*mut UObject>()
                .read_unaligned()
        };
        let raw_fname: u64 = unsafe {
            let fname_ptr = effect.field_ptr(STATUS_EFFECT_ROW_HANDLE + 8);
            (fname_ptr as *const u64).read_unaligned()
        };
        let row_name = unsafe {
            ueforge::ue::runtime()
                .name_resolver
                .to_string(ueforge::ue::FName::from_u64(raw_fname))
        };
        let table_name = unsafe {
            table_ptr
                .as_ref()
                .map(|t| t.full_name())
                .unwrap_or_else(|| "<null-table>".to_string())
        };
        let row_meta = unsafe {
            table_ptr
                .as_ref()
                .and_then(|t| lookup_data_table_row(t, raw_fname).map(read_status_effect_row))
        };
        let (stat_type, value) = match row_meta {
            Some((ty, val)) => (Some(ty), Some(val)),
            None => (None, None),
        };
        entries.push(StatusEffectEntry {
            row: row_name,
            table: table_name,
            stat_type,
            value,
        });
    }
    entries
}
