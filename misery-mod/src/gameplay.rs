//! Read and write S_GameplaySettings on BP_SGKGameInstance_C.
//!
//! The struct is inline at offset 0x218 on the GameInstance,
//! NOT on BP_GlobalManager_C (which was the original wrong
//! assumption). Field offsets within the struct are from the
//! UE4SS object dump. See docs/research.md section 8.6.

use ueforge::ue;
pub use ueforge::ue::struct_fields::{FieldAccessor, FieldDef, FieldType, FieldValue};

const STRUCT_BASE: usize = 0x218;
const GI_CLASS: &str = "BP_SGKGameInstance_C";

pub static FIELDS: &[FieldDef] = &[
    FieldDef { name: "ShiningsTimer",              desc: "Minutes per emission cycle",                          offset: 0x00, ty: FieldType::Double },
    FieldDef { name: "DayLength",                  desc: "Duration of daytime in minutes",                      offset: 0x08, ty: FieldType::Double },
    FieldDef { name: "NightLength",                desc: "Duration of nighttime in minutes",                    offset: 0x10, ty: FieldType::Double },
    FieldDef { name: "WeatherCycleDuration",       desc: "How long each weather cycle lasts",                   offset: 0x18, ty: FieldType::Double },
    FieldDef { name: "InitialSeason",              desc: "Starting season index",                               offset: 0x20, ty: FieldType::Double },
    FieldDef { name: "HungerSpeed",                desc: "How fast hunger drains (higher = faster)",            offset: 0x28, ty: FieldType::Double },
    FieldDef { name: "ThirstSpeed",                desc: "How fast thirst drains (higher = faster)",            offset: 0x30, ty: FieldType::Double },
    FieldDef { name: "StaminaDrainRate",           desc: "How fast stamina depletes (higher = faster)",         offset: 0x38, ty: FieldType::Double },
    FieldDef { name: "HeadshotDamageMultiplier",   desc: "Damage multiplier for headshots",                    offset: 0x40, ty: FieldType::Double },
    FieldDef { name: "DamageMultiplier",           desc: "Global player damage output multiplier",              offset: 0x48, ty: FieldType::Double },
    FieldDef { name: "ItemsDurabilityDamageMultiplier", desc: "How fast items lose durability",                 offset: 0x50, ty: FieldType::Double },
    FieldDef { name: "AnomaliesDamageToPlayer",    desc: "Damage anomalies deal to the player",                offset: 0x58, ty: FieldType::Double },
    FieldDef { name: "AnomaliesSpawnRate",         desc: "How often anomalies spawn (higher = more)",          offset: 0x60, ty: FieldType::Double },
    FieldDef { name: "EnemySpawnRate",             desc: "How often enemies spawn (higher = more)",            offset: 0x68, ty: FieldType::Double },
    FieldDef { name: "EnemyDamageToPlayer",        desc: "Damage enemies deal to the player",                  offset: 0x70, ty: FieldType::Double },
    FieldDef { name: "EnemySpeed",                 desc: "Enemy movement speed multiplier",                    offset: 0x78, ty: FieldType::Double },
    FieldDef { name: "RadiationPower",             desc: "Radiation zone strength",                            offset: 0x80, ty: FieldType::Double },
    FieldDef { name: "InsanityPower",              desc: "Insanity buildup rate",                              offset: 0x88, ty: FieldType::Double },
    FieldDef { name: "AmmoScarcity",               desc: "Ammo spawn rarity (higher = less ammo)",             offset: 0x90, ty: FieldType::Double },
    FieldDef { name: "FoodScarcity",               desc: "Food spawn rarity (higher = less food)",             offset: 0x98, ty: FieldType::Double },
    FieldDef { name: "HealsScarcity",              desc: "Healing item rarity (higher = fewer heals)",         offset: 0xA0, ty: FieldType::Double },
    FieldDef { name: "RespawnHealthMultiplier",    desc: "Health fraction after respawning (1.0 = full)",      offset: 0xA8, ty: FieldType::Double },
    FieldDef { name: "WeightLimitMultiplier",      desc: "Carry weight limit multiplier (higher = carry more)", offset: 0xB0, ty: FieldType::Double },
    FieldDef { name: "FriendlyFire",               desc: "Players can damage each other",                      offset: 0xB8, ty: FieldType::Bool },
    FieldDef { name: "Shitting",                   desc: "Defecation mechanic enabled",                        offset: 0xB9, ty: FieldType::Bool },
    FieldDef { name: "Permadeath",                 desc: "Death is permanent (no respawn)",                    offset: 0xBA, ty: FieldType::Bool },
    FieldDef { name: "RespawnOnEmission",          desc: "Player respawns when an emission hits",              offset: 0xBB, ty: FieldType::Bool },
    FieldDef { name: "CollisionBetweenPlayers",    desc: "Players physically collide with each other",         offset: 0xBC, ty: FieldType::Bool },
];

/// Opens MISERY's live gameplay settings so the mod can read or change them.
/// Stays here because the owning Blueprint and struct offset are MISERY-specific; Ueforge owns the accessor.
pub fn accessor() -> Result<FieldAccessor, String> {
    let ptr = ue::actor::find_object(GI_CLASS, None, false)
        .ok_or_else(|| "no game instance found".to_string())?;
    Ok(FieldAccessor::new(ptr, STRUCT_BASE, "gameplay"))
}

// ---- UI ----

struct CachedState {
    loaded: bool,
    doubles: Vec<f32>,
    bools: Vec<bool>,
}

static UI_STATE: std::sync::OnceLock<std::sync::Mutex<CachedState>> =
    std::sync::OnceLock::new();

/// Keeps the Gameplay tab's editable values stable between frames.
/// Stays here because this cache mirrors MISERY's settings list and presentation.
fn ui_state() -> &'static std::sync::Mutex<CachedState> {
    UI_STATE.get_or_init(|| {
        let n = FIELDS.len();
        std::sync::Mutex::new(CachedState {
            loaded: false,
            doubles: vec![0.0; n],
            bools: vec![false; n],
        })
    })
}

/// Refreshes every displayed gameplay setting from the running MISERY session.
/// Stays here because it reads MISERY's known settings fields through Ueforge's generic accessor.
fn load_all(s: &mut CachedState) -> Result<(), String> {
    let acc = accessor()?;
    for (i, field) in FIELDS.iter().enumerate() {
        let abs = acc.base_offset() + field.offset;
        match field.ty {
            FieldType::Double => {
                let v: f64 = unsafe { (acc.ptr().add(abs) as *const f64).read_unaligned() };
                s.doubles[i] = v as f32;
            }
            FieldType::Bool => {
                let v: u8 = unsafe { *acc.ptr().add(abs) };
                s.bools[i] = v != 0;
            }
        }
    }
    s.loaded = true;
    Ok(())
}

/// Gives each MISERY setting a useful slider range while keeping the current value reachable.
/// Stays here because these ranges are player-facing balance choices for this game.
fn slider_range(name: &str, current: f32) -> (f32, f32) {
    let (lo, hi) = match name {
        "ShiningsTimer" => (0.0, 120.0),
        "DayLength" | "NightLength" => (0.0, 60.0),
        "WeatherCycleDuration" => (0.0, 120.0),
        "InitialSeason" => (0.0, 4.0),
        "RespawnHealthMultiplier" => (0.0, 2.0),
        _ => (0.0, 10.0),
    };
    let hi = if current > hi { current * 2.0 } else { hi };
    (lo, hi)
}

/// Draws the Gameplay tab where players can tune MISERY's live rules.
/// Stays here because the tab presents MISERY fields; Ueforge owns only the reusable UI controls.
pub fn render() {
    use ueforge::ui;

    ui::text("Gameplay settings");
    ui::text_disabled("Drag sliders or toggle checkboxes. Click Refresh after loading a save.");
    ui::spacing();

    let Ok(mut s) = ui_state().lock() else { return };

    if ui::button("Refresh") {
        let _ = load_all(&mut s);
    }

    if !s.loaded {
        ui::text_disabled("Click Refresh after loading a save.");
        return;
    }

    ui::separator();
    ui::spacing();
    ui::begin_child("##gp_scroll", 0.0, 0.0);

    for (i, field) in FIELDS.iter().enumerate() {
        match field.ty {
            FieldType::Double => {
                let (lo, hi) = slider_range(field.name, s.doubles[i]);
                ui::text(field.name);
                ui::same_line();
                ui::text_disabled(field.desc);
                ui::set_next_item_width(250.0);
                let label = format!("##gp_{}", field.name);
                if ui::slider_f32(&label, &mut s.doubles[i], lo, hi) {
                    if let Ok(acc) = accessor() { acc.write_double(field, s.doubles[i] as f64); }
                }
            }
            FieldType::Bool => {
                let label = format!("{}##gp_b", field.name);
                if ui::checkbox(&label, &mut s.bools[i]) {
                    if let Ok(acc) = accessor() { acc.write_bool(field, s.bools[i]); }
                }
                ui::same_line();
                ui::text_disabled(field.desc);
            }
        }
        ui::spacing();
    }

    ui::dummy(0.0, 40.0);
    ui::end_child();
}
