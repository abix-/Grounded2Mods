//! Wild West Miner skill catalog + HTTP ops + Harmony hooks.
//!
//! Catalog is fully declarative: each `SkillDef` references a
//! `UnityField*Effect` static so a level-change writes the
//! corresponding singleton field on the active slot. State
//! mutation goes through [`modforge::rpg::Tracker`] which
//! transactionally persists to disk under
//! `<DLL_dir>/wwm-mod/<slot>.json`.
//!
//! The two Harmony postfixes grant XP per dig swing and per
//! currency event. `TRACKER.record_xp` does the curve math,
//! awards skill points on level-up, and persists.

use std::ffi::c_void;
use std::sync::OnceLock;
use std::time::Duration;

use modforge::rpg::poller::{PollerHandle, SlotPoller};
use modforge::rpg::vanilla::VanillaCache;
use modforge::rpg::xp::Curve;

use unityforge::hook::{HOOK_REGISTRY, patch_postfix};
use unityforge::rpg::{
    SkillDef, SkillRegistry, Tracker, UnityFieldAdditiveEffect, UnityFieldMultiplyEffect,
    UnitySlotKey,
};

// ---- Effects --------------------------------------------------------

// Each effect needs a `'static` VanillaCache so the engine's
// first-seen value is captured once and used as the baseline
// for level/refund/toggle. Field names are best guesses from
// the WWM research doc; verify with `walk_class` once the
// plugin is running.

static STRONG_BACK_VANILLA: VanillaCache<&'static str, f32> = VanillaCache::new();
static STRONG_BACK_EFFECT: UnityFieldAdditiveEffect = UnityFieldAdditiveEffect::new(
    "PlayerCarryingController",
    "_maxCapacity",
    50.0,
    "carry capacity",
    &STRONG_BACK_VANILLA,
);

static GREEDY_MINER_VANILLA: VanillaCache<&'static str, f32> = VanillaCache::new();
static GREEDY_MINER_EFFECT: UnityFieldMultiplyEffect = UnityFieldMultiplyEffect::new(
    "MineDataSO",
    "_oreValue",
    1.0,
    "gold per ore",
    &GREEDY_MINER_VANILLA,
);

static QUICK_PICKAXE_VANILLA: VanillaCache<&'static str, f32> = VanillaCache::new();
static QUICK_PICKAXE_EFFECT: UnityFieldMultiplyEffect = UnityFieldMultiplyEffect::new(
    "DigManager",
    "_digRange",
    0.5,
    "dig range",
    &QUICK_PICKAXE_VANILLA,
);

static CHARISMA_VANILLA: VanillaCache<&'static str, f32> = VanillaCache::new();
static CHARISMA_EFFECT: UnityFieldMultiplyEffect = UnityFieldMultiplyEffect::new(
    "WorkersManager",
    "_hireCostMultiplier",
    -0.5,
    "hire cost",
    &CHARISMA_VANILLA,
);

static RESILIENT_VANILLA: VanillaCache<&'static str, f32> = VanillaCache::new();
static RESILIENT_EFFECT: UnityFieldMultiplyEffect = UnityFieldMultiplyEffect::new(
    "PlayerStaminaController",
    "_staminaDrainMultiplier",
    -0.5,
    "stamina drain",
    &RESILIENT_VANILLA,
);

// Lucky uses a RuntimeEffect: the actual probability-scaling
// behavior lives in a hot-path Harmony postfix that reads the
// skill level on every fire. The Effect impl is a no-op apply
// + a formatter that renders "+X% lucky drops" in the ImGui
// tab.
use modforge::rpg::format::PercentFormat;
use modforge::rpg::std_effect::RuntimeEffect;
static LUCKY_EFFECT: RuntimeEffect = RuntimeEffect {
    max_bonus: 0.5,
    format: PercentFormat::PlusPercent {
        word: "lucky drops",
    },
};

// ---- Catalog --------------------------------------------------------

pub static CATALOG: SkillRegistry = SkillRegistry::new(&[
    SkillDef {
        id: "strong_back",
        display_name: "Strong Back",
        max_level: 10,
        effect: modforge::rpg::EffectDef::new("UnityFieldAdditive", &STRONG_BACK_EFFECT),
        trigger: &modforge::rpg::ON_SLOT_CHANGE,
    },
    SkillDef {
        id: "greedy_miner",
        display_name: "Greedy Miner",
        max_level: 10,
        effect: modforge::rpg::EffectDef::new("UnityFieldMultiply", &GREEDY_MINER_EFFECT),
        trigger: &modforge::rpg::ON_SLOT_CHANGE,
    },
    SkillDef {
        id: "quick_pickaxe",
        display_name: "Quick Pickaxe",
        max_level: 10,
        effect: modforge::rpg::EffectDef::new("UnityFieldMultiply", &QUICK_PICKAXE_EFFECT),
        trigger: &modforge::rpg::ON_SLOT_CHANGE,
    },
    SkillDef {
        id: "charisma",
        display_name: "Charisma",
        max_level: 10,
        effect: modforge::rpg::EffectDef::new("UnityFieldMultiply", &CHARISMA_EFFECT),
        trigger: &modforge::rpg::ON_SLOT_CHANGE,
    },
    SkillDef {
        id: "resilient",
        display_name: "Resilient",
        max_level: 10,
        effect: modforge::rpg::EffectDef::new("UnityFieldMultiply", &RESILIENT_EFFECT),
        trigger: &modforge::rpg::ON_SLOT_CHANGE,
    },
    SkillDef {
        id: "lucky",
        display_name: "Lucky",
        max_level: 10,
        effect: modforge::rpg::EffectDef::new("Runtime", &LUCKY_EFFECT),
        trigger: &modforge::rpg::ON_SLOT_CHANGE,
    },
]);

// ---- Tracker --------------------------------------------------------

pub static TRACKER: Tracker = Tracker::new(&CATALOG, Curve::new(100.0, 1.8, 50), "wwm-mod");

// ---- Slot poller ----------------------------------------------------

static SLOT_KEY: UnitySlotKey =
    UnitySlotKey::new("GameSerializationSystem", "_currentLoadedSaveNumber");

static POLLER: OnceLock<PollerHandle> = OnceLock::new();

// ---- Install --------------------------------------------------------

/// Render fn invoked by the `render_tab` op for the "RPG"
/// tab. Logs a snapshot of the catalog + state to the BepInEx
/// log sink. Once an in-process ImGui binding lands on
/// unityforge, this will draw widgets directly; today it's a
/// log line so the user can see the tab is wired and the
/// catalog state.
pub fn render_tab() {
    let snapshot = TRACKER.with_state(|s| {
        let mut line = format!(
            "RPG: xp={} level={} points={}",
            s.xp, s.level, s.skill_points
        );
        for skill in CATALOG.iter() {
            let lv = s.level_of(skill.id);
            let txt = TRACKER.format_effect(skill, lv);
            line.push_str(&format!(
                "\n  {} L{}/{}  {}",
                skill.display_name, lv, skill.max_level, txt
            ));
        }
        line
    });
    let line = snapshot.unwrap_or_else(|| "RPG: <no slot active>".to_string());
    unityforge::mono::log(unityforge::mono::LogLevel::Info, &line);
}

pub fn install() {
    unityforge::rpg::ops::register(&TRACKER);
    install_hooks();
    spawn_slot_poller();
}

fn spawn_slot_poller() {
    let handle = SlotPoller::spawn(
        Duration::from_secs(1),
        || SLOT_KEY.resolve(),
        |slot| TRACKER.activate_slot(slot),
        || TRACKER.deactivate_slot(),
    );
    let _ = POLLER.set(handle);
}

fn install_hooks() {
    // Harmony postfix on DigManager.Dig. Each swing awards 5 XP.
    // Real method name + signature TBD via `walk_class DigManager`
    // once the plugin is loaded; the framework will surface the
    // patch error if the target isn't found.
    if let Ok(hook) = patch_postfix("DigManager", "Dig", on_dig_postfix) {
        HOOK_REGISTRY.register(hook);
    }
    // Harmony postfix on PlayerManager.AddPlayerCurrency. Each
    // currency event awards 10 XP.
    if let Ok(hook) = patch_postfix("PlayerManager", "AddPlayerCurrency", on_currency_postfix) {
        HOOK_REGISTRY.register(hook);
    }
}

extern "C" fn on_dig_postfix(_ctx: *const c_void) {
    let _ = TRACKER.record_xp(5);
}

extern "C" fn on_currency_postfix(_ctx: *const c_void) {
    let _ = TRACKER.record_xp(10);
}
