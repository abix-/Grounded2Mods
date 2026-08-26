//! Settlement upgrade state and the C# effect bridge.

use std::ffi::{CStr, CString, c_char};
use std::path::PathBuf;

use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use modforge::upgrade::UpgradeStore;
use unityforge::mono;

use crate::common::on_main_thread;

const PROPS: &str = "props";
const COMMUNITIES: &str = "communities";

static UPGRADES: UpgradeStore = UpgradeStore::new(1);

fn scope(scope: i32) -> Option<&'static str> {
    match scope {
        0 => Some(PROPS),
        1 => Some(COMMUNITIES),
        _ => None,
    }
}

fn with_text<T>(ptr: *const c_char, f: impl FnOnce(&str) -> T) -> Option<T> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: C# supplies a non-null, NUL-terminated string for the
    // duration of each synchronous call.
    unsafe { CStr::from_ptr(ptr) }.to_str().ok().map(f)
}

fn path(ptr: *const u16, len: i32) -> Option<PathBuf> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: C# supplies `len` UTF-16 code units that remain valid
    // for the duration of this synchronous call.
    let units = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    String::from_utf16(units).ok().map(PathBuf::from)
}

#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_load(seed: i64, path_ptr: *const u16, path_len: i32) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(path) = path(path_ptr, path_len) else { return -1 };
        let existed = path.exists();
        match UPGRADES.load(seed, path) {
            Ok(()) => {
                let _ = UPGRADES.ensure_scope(PROPS);
                let _ = UPGRADES.ensure_scope(COMMUNITIES);
                if existed {
                    mono::log(
                        mono::LogLevel::Info,
                        &format!(
                            "SettlementUpgrades: restored upgrades for {} structure(s) and {} camp(s) (seed {seed})",
                            UPGRADES.status(PROPS).entities_upgraded,
                            UPGRADES.status(COMMUNITIES).entities_upgraded,
                        ),
                    );
                }
                0
            }
            Err(error) => {
                let _ = UPGRADES.ensure_scope(PROPS);
                let _ = UPGRADES.ensure_scope(COMMUNITIES);
                mono::log(
                    mono::LogLevel::Warn,
                    &format!("SettlementUpgrades: sidecar load failed: {error}"),
                );
                -1
            }
        }
    }))
    .unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_get(
    scope_id: i32,
    entity_id: i32,
    track: *const c_char,
) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(scope) = scope(scope_id) else {
            return 0;
        };
        with_text(track, |track| {
            UPGRADES.level(scope, i64::from(entity_id), track) as i32
        })
        .unwrap_or(0)
    }))
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_set(
    scope_id: i32,
    entity_id: i32,
    track: *const c_char,
    level: i32,
) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(scope) = scope(scope_id) else {
            return -1;
        };
        let Some(result) = with_text(track, |track| {
            UPGRADES.set_level(scope, i64::from(entity_id), track, i64::from(level))
        }) else {
            return -1;
        };
        match result {
            Ok(()) => 0,
            Err(error) => {
                mono::log(
                    mono::LogLevel::Warn,
                    &format!("SettlementUpgrades: sidecar write failed: {error}"),
                );
                -1
            }
        }
    }))
    .unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_has_any(scope_id: i32, track: *const c_char) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(scope) = scope(scope_id) else {
            return 0;
        };
        with_text(
            track,
            |track| {
                if UPGRADES.has_any(scope, track) { 1 } else { 0 }
            },
        )
        .unwrap_or(0)
    }))
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_cost(base_need: f64, factor: i32, next_level: i32) -> i32 {
    std::panic::catch_unwind(|| {
        modforge::upgrade::cost(base_need, i64::from(factor), i64::from(next_level)) as i32
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_skill(
    base: i32,
    levels_per_band: i32,
    next_level: i32,
) -> i32 {
    std::panic::catch_unwind(|| {
        modforge::upgrade::skill_requirement(
            i64::from(base),
            i64::from(levels_per_band),
            i64::from(next_level),
        ) as i32
    })
    .unwrap_or(base)
}

#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_curve(level: i32, base_step: f32, decay: f32) -> f32 {
    std::panic::catch_unwind(|| {
        modforge::upgrade::diminishing_bonus(i64::from(level), base_step, decay)
    })
    .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_status() -> *mut c_char {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let status = UPGRADES.status(PROPS);
        let report = json!({
            "structures_upgraded": status.entities_upgraded,
            "levels_total": status.levels_total,
            "levels_per_track": status.levels_per_track,
            "seed": UPGRADES.slot().unwrap_or(0),
        })
        .to_string();
        CString::new(report)
            .map(CString::into_raw)
            .unwrap_or(std::ptr::null_mut())
    }))
    .unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_string_free(ptr: *mut c_char) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if !ptr.is_null() {
            // SAFETY: the pointer came from CString::into_raw in
            // survivalist_upgrade_status and is freed exactly once.
            drop(unsafe { CString::from_raw(ptr) });
        }
    }));
}

pub fn register_ops() {
    OP_REGISTRY.register_many([
        OpDef::new(
            "upgrade_probe",
            "Task-1 gate probe: set Reinforce level N on the player camp's first structure of the named type and read max hit points back through the game. {type: str, level: number}",
            "{type: str, level: number}",
            upgrade_probe,
        ),
        OpDef::new(
            "upgrade_status",
            "Settlement upgrades: structures upgraded, levels per track, total levels.",
            "{}",
            upgrade_status,
        ),
    ]);
}

/// The C# entries return a JSON string; unwrap it into a real
/// object so op results read clean.
fn parse_report(v: Json) -> Result<Json, String> {
    match v {
        Json::String(s) => {
            serde_json::from_str(&s).map_err(|e| format!("bad probe report json: {e}"))
        }
        other => Ok(other),
    }
}

fn upgrade_probe(args: &Json) -> Result<Json, String> {
    let type_name = args
        .get("type")
        .and_then(Json::as_str)
        .ok_or("missing arg 'type' (prop prototype name, e.g. WoodenChest)")?
        .to_string();
    let level = args.get("level").and_then(Json::as_i64).unwrap_or(1);
    on_main_thread(move || {
        let v = mono::invoke_static(
            "SettlementUpgrades",
            "UpgradeProbe",
            &json!([type_name, level]),
        )?;
        parse_report(v)
    })
}

fn upgrade_status(_args: &Json) -> Result<Json, String> {
    on_main_thread(|| {
        let v = mono::invoke_static("SettlementUpgrades", "Status", &json!([]))?;
        parse_report(v)
    })
}
