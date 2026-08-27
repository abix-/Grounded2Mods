// Hunger and thirst rate patcher. Two `ueforge::tweak::TweakDef`
// entries on `USurvivalComponent` instances. The framework owns
// vanilla capture + idempotent re-apply + the multiplier atomic;
// this file owns the field NAMES and the call from `worker()`.
//
// USurvivalComponent (Maine.SurvivalComponent) holds:
//   FSurvivalTraitSettings HungerSettings;  // +0x0138
//   FSurvivalTraitSettings ThirstSettings;  // +0x0180
// Inside FSurvivalTraitSettings: float AdjustmentPerSecond at +0x08.
//
// Note: TweakDef::class_f32 resolves the offset by field name from
// the discovery cache. The "HungerSettings_AdjustmentPerSecond"
// shape below assumes those names are reflected; if the discovery
// dump shows a different name we update here without touching any
// offsets.

use ueforge::tweak::{TweakDef, TweakOp};

// Offsets kept as constants so the rest of g2rpg can reference
// them for raw reads (debug snapshot, capture_vanilla, etc.).
pub const HUNGER_ADJUSTMENT_OFFSET: usize = 0x0138 + 0x08; // 0x0140
pub const THIRST_ADJUSTMENT_OFFSET: usize = 0x0180 + 0x08; // 0x0188

static HUNGER_TWEAK: TweakDef = TweakDef::class_f32(
    "hunger",
    "SurvivalComponent",
    "HungerSettings.AdjustmentPerSecond",
    TweakOp::Multiply,
    1.0,
);
static THIRST_TWEAK: TweakDef = TweakDef::class_f32(
    "thirst",
    "SurvivalComponent",
    "ThirstSettings.AdjustmentPerSecond",
    TweakOp::Multiply,
    1.0,
);

pub struct SurvivalStats {
    pub scanned_classes: usize,
    pub patched: usize,
}

/// Applies the configured hunger and thirst rates to Grounded 2's survival components.
/// Stays here because the setting choices and survival field names belong to Grounded 2;
/// Ueforge owns vanilla capture and idempotent field tweaking.
pub fn run(thirst_multiplier: f32, hunger_multiplier: f32) -> SurvivalStats {
    HUNGER_TWEAK.store_f32(hunger_multiplier);
    THIRST_TWEAK.store_f32(thirst_multiplier);

    let mut patched = 0usize;
    let mut scanned_classes = 0usize;
    for (id, t) in [("hunger", &HUNGER_TWEAK), ("thirst", &THIRST_TWEAK)] {
        match t.apply() {
            Ok(n) => {
                patched += n;
                scanned_classes = scanned_classes.max(t.vanilla_count());
                if n == 0 {
                    ueforge::log!(
                        "survival: '{id}' multiplier is 1.0 (or skipped), no patch"
                    );
                }
            }
            Err(e) => ueforge::log!("survival: '{id}' apply failed: {e}"),
        }
    }

    SurvivalStats {
        scanned_classes,
        patched,
    }
}
