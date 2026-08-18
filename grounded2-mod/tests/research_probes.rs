// Research probes: archived test-side reimplementation of the
// fall_hook diagnostic probes that used to live in the mod.
//
// The mod stays generic. All shared "find live player +
// follow component pointer + read field" plumbing lives in
// `ueforge::client`. This file owns only the
// G2-specific class names + offsets + interpretation.
//
// Original mod-side functions removed in 2026-05-09 evening's
// performance pass (they fired on every UFunction call to the
// player BP class. 1090+/sec. Causing 50% CPU and growing
// RAM through allocator churn). See `../docs/ongoing.md` section
// 13 "Measured runaway".
//
// Each probe below is `#[ignore]`d by default. Run individually:
//   set BBP_DEBUG_PORT=17171
//   cargo test --target x86_64-pc-windows-msvc \
//     --test research_probes <probe_name>. --nocapture --ignored

#![allow(dead_code)]

mod common;

use ueforge::client;

// ---------------------------------------------------------------
// Field offsets and class names. Captured from the mod-side
// research code; preserved here so the tests have the same
// ground truth.
// ---------------------------------------------------------------

const PLAYER_CLASS: &str = "BP_SurvivalPlayerCharacter_Female02_C";
const ASC_STATUS_EFFECT_COMPONENT: u64 = 0x1378;
const ASC_HEALTH_COMPONENT: u64 = 0x1340;
const ASC_CHAR_MOVEMENT_COMPONENT: u64 = 0x1380;
const HC_CURRENT_DAMAGE: u64 = 0x032C;
const CMC_VELOCITY_Z: u64 = 0x00E8;

// EStatusEffectType values from Maine_structs.hpp:136 that are
// relevant to skills in our catalog.
const PROBE_STAT_TYPES: &[(&str, u8)] = &[
    ("FallDamage", 14),
    ("DamageReduction", 29),
    ("DamageReductionMultiplier", 30),
    ("AttackDamage", 23),
    ("LifeSteal", 38),
    ("CriticalHitChance", 31),
    ("CriticalDamage", 62),
    ("ReflectDamage", 37),
    ("MaxHealth", 5),
];

// ---------------------------------------------------------------
// Probe 1: dump player StatusEffects array with row Type/Value.
// Snapshot already exposes status_effects; this just reads them.
// ---------------------------------------------------------------

#[test]
#[ignore]
fn probe_player_status_effects() {
    let api = common::Api::require();
    let s = api.snapshot();
    let sfx = s.status_effects.as_deref().unwrap_or(&[]);
    eprintln!("player has {} active status effects", sfx.len());
    for (i, e) in sfx.iter().enumerate() {
        eprintln!(
            "  [{i:2}] row={:<28} type={:?} value={:?}  table={}",
            e.row, e.stat_type, e.value, e.table
        );
    }
}

// ---------------------------------------------------------------
// Probe 2: GetValueForStat() per damage-relevant stat type.
// Calls UStatusEffectComponent::GetValueForStat(StatType, false)
// for each interesting type via call_ufunction_typed. Logs the
// engine's combined value across all active effects.
// ---------------------------------------------------------------

#[repr(C)]
#[derive(
    Default,
    Copy,
    Clone,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::Immutable,
    zerocopy::KnownLayout,
)]
struct GetValueForStatParms {
    stat_type: u8,
    /// UE bool. Stored as u8 here because zerocopy::FromBytes
    /// rejects `bool` (not every byte pattern is a valid bool
    /// in Rust). Engine writes 0 or 1; treat non-zero as true
    /// in callers.
    temporary_only: u8,
    _pad: [u8; 2],
    return_value: f32,
}

#[test]
#[ignore]
fn probe_status_effect_values() {
    let api = common::Api::require();
    let player = client::find_live_instance(api.inner(), PLAYER_CLASS)
        .expect("no live player; load a save and spawn");
    let sec_addr = client::read_component_ptr(
        api.inner(),
        player.addr,
        ASC_STATUS_EFFECT_COMPONENT,
    )
    .expect("StatusEffectComponent ptr is null");
    let sec_sel = format!("addr:0x{sec_addr:X}");

    eprintln!("rpg/sfx-probe (test-side):");
    let mut summary = String::new();
    for (name, stat_type) in PROBE_STAT_TYPES {
        let parms = GetValueForStatParms {
            stat_type: *stat_type,
            temporary_only: 0,
            _pad: [0; 2],
            return_value: 0.0,
        };
        match api.inner().call_ufunction_typed::<GetValueForStatParms>(
            "StatusEffectComponent",
            "GetValueForStat",
            &sec_sel,
            parms,
        ) {
            Ok((after, _state)) => {
                summary.push_str(&format!(" {}({})={:.3}", name, stat_type, after.return_value));
            }
            Err(e) => {
                summary.push_str(&format!(" {}({})=ERR({})", name, stat_type, e));
            }
        }
    }
    eprintln!("  {}", summary);
}

// ---------------------------------------------------------------
// Probe 3: read player CurrentDamage live.
// Replaced by snapshot.live_player.hc.current_damage.
// ---------------------------------------------------------------

#[test]
#[ignore]
fn probe_player_current_damage() {
    let api = common::Api::require();
    let s = api.snapshot();
    let hc = s.live_hc();
    eprintln!(
        "player CurrentDamage = {:.2} / MaxHealth {:.2}",
        hc.current_damage, hc.max_health
    );
}

// ---------------------------------------------------------------
// Probe 4: read player CharMovementComponent.Velocity.Z live.
// ---------------------------------------------------------------

#[test]
#[ignore]
fn probe_player_velocity_z() {
    let api = common::Api::require();
    let player = client::find_live_instance(api.inner(), PLAYER_CLASS)
        .expect("no live player");
    let cmc_addr = client::read_component_ptr(
        api.inner(),
        player.addr,
        ASC_CHAR_MOVEMENT_COMPONENT,
    )
    .expect("CMC ptr is null");
    let vz = client::read_f32(api.inner(), cmc_addr, CMC_VELOCITY_Z);
    eprintln!("player Velocity.Z = {:.2}", vz);
}

// ---------------------------------------------------------------
// `is_collision_relevant` from the mod is preserved here as a
// pure function. Original used to gate the `impact-trace` PE
// log block in fall_hook. Now lives test-side because that
// trace was 100% research scaffolding.
// ---------------------------------------------------------------

pub fn is_collision_relevant(fn_name: &str) -> bool {
    if fn_name.contains("Tick") {
        return false;
    }
    fn_name.contains("Damage")
        || fn_name.contains("Damaged")
        || fn_name.contains("Hit")
        || fn_name.contains("Land")
        || fn_name.contains("Fall")
        || fn_name.contains("Impact")
        || fn_name.contains("Health")
        || fn_name.contains("ReceiveAny")
        || fn_name.contains("ReceiveActor")
}
