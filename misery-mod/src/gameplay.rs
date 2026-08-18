//! Read and write S_GameplaySettings on BP_SGKGameInstance_C.
//!
//! The struct is inline at offset 0x218 on the GameInstance,
//! NOT on BP_GlobalManager_C (which was the original wrong
//! assumption). Field offsets within the struct are from the
//! UE4SS object dump. See docs/misery-research.md section 8.6.

use ueforge::ue;
use ueforge::ue::{read_at, write_at};

pub const STRUCT_BASE: usize = 0x218;
const GI_CLASS: &str = "BP_SGKGameInstance_C";

#[derive(Debug, Clone, Copy)]
pub enum FieldType {
    Double,
    Bool,
}

#[derive(Debug, Clone, Copy)]
pub struct FieldDef {
    pub name: &'static str,
    pub desc: &'static str,
    pub offset: usize,
    pub ty: FieldType,
}

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

#[derive(Debug, Clone)]
pub enum FieldValue {
    Double(f64),
    Bool(bool),
}

impl std::fmt::Display for FieldValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldValue::Double(v) => write!(f, "{v}"),
            FieldValue::Bool(v) => write!(f, "{v}"),
        }
    }
}

pub fn game_instance_ptr() -> Result<*const u8, String> {
    ue::actor::find_object(GI_CLASS, None, false)
        .ok_or_else(|| "no game instance found".into())
}

pub fn read_field(field: &FieldDef) -> Result<FieldValue, String> {
    let ptr = game_instance_ptr()?;
    let abs = STRUCT_BASE + field.offset;
    match field.ty {
        FieldType::Double => {
            let v: f64 = unsafe { read_at(ptr, abs) };
            Ok(FieldValue::Double(v))
        }
        FieldType::Bool => {
            let v: u8 = unsafe { read_at(ptr, abs) };
            Ok(FieldValue::Bool(v != 0))
        }
    }
}

pub fn write_double(field: &FieldDef, value: f64) -> Result<(), String> {
    let ptr = game_instance_ptr()?;
    unsafe { write_at(ptr, STRUCT_BASE + field.offset, value) };
    ueforge::log::log(format_args!(
        "gameplay: {} = {value}", field.name
    ));
    Ok(())
}

pub fn write_bool(field: &FieldDef, value: bool) -> Result<(), String> {
    let ptr = game_instance_ptr()?;
    unsafe { write_at(ptr, STRUCT_BASE + field.offset, value as u8) };
    ueforge::log::log(format_args!(
        "gameplay: {} = {value}", field.name
    ));
    Ok(())
}
