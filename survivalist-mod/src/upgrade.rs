//! Settlement upgrade state and the C# effect bridge.

use std::ffi::c_char;

use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use modforge::upgrade::UpgradeStore;
use unityforge::ffi;
use unityforge::main_thread_queue::MAIN_QUEUE;
use unityforge::mono;

const PROPS: &str = "props";
const COMMUNITIES: &str = "communities";

static UPGRADES: UpgradeStore = UpgradeStore::new(1);

/// Translate the C# scope number into structure or settlement upgrade state.
/// Stays here because it implements Survivalist upgrade scopes and the exact C# shim contract; Modforge owns upgrade state and math.
fn scope(scope: i32) -> Option<&'static str> {
    match scope {
        0 => Some(PROPS),
        1 => Some(COMMUNITIES),
        _ => None,
    }
}

/// Load this world's upgrade state for the C# effects layer.
/// Stays here because it implements Survivalist upgrade scopes and the exact C# shim contract; Modforge owns upgrade state and math.
#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_load(seed: i64, path_ptr: *const u16, path_len: i32) -> i32 {
    ffi::catch_result_or(-1, || {
        // SAFETY: C# keeps the supplied UTF-16 buffer live for this call.
        let path = (unsafe { ffi::utf16_path(path_ptr, path_len) }).ok_or(())?;
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
                Ok(0)
            }
            Err(error) => {
                let _ = UPGRADES.ensure_scope(PROPS);
                let _ = UPGRADES.ensure_scope(COMMUNITIES);
                mono::log(
                    mono::LogLevel::Warn,
                    &format!("SettlementUpgrades: sidecar load failed: {error}"),
                );
                Err(())
            }
        }
    })
}

/// Return one structure or settlement upgrade level to C#.
/// Stays here because it implements Survivalist upgrade scopes and the exact C# shim contract; Modforge owns upgrade state and math.
#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_get(
    scope_id: i32,
    entity_id: i32,
    track: *const c_char,
) -> i32 {
    ffi::catch_or(0, || {
        let Some(scope) = scope(scope_id) else {
            return 0;
        };
        // SAFETY: C# keeps the supplied NUL-terminated track name live for this call.
        unsafe {
            ffi::with_utf8(track, |track| {
                UPGRADES.level(scope, i64::from(entity_id), track) as i32
            })
        }
        .unwrap_or(0)
    })
}

/// Persist a purchased structure or settlement upgrade level.
/// Stays here because it implements Survivalist upgrade scopes and the exact C# shim contract; Modforge owns upgrade state and math.
#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_set(
    scope_id: i32,
    entity_id: i32,
    track: *const c_char,
    level: i32,
) -> i32 {
    ffi::catch_result_or(-1, || {
        let scope = scope(scope_id).ok_or(())?;
        // SAFETY: C# keeps the supplied NUL-terminated track name live for this call.
        let result = (unsafe {
            ffi::with_utf8(track, |track| {
                UPGRADES.set_level(scope, i64::from(entity_id), track, i64::from(level))
            })
        })
        .ok_or(())?;
        match result {
            Ok(()) => Ok(0),
            Err(error) => {
                mono::log(
                    mono::LogLevel::Warn,
                    &format!("SettlementUpgrades: sidecar write failed: {error}"),
                );
                Err(())
            }
        }
    })
}

/// Tell C# whether any entity owns a named upgrade track.
/// Stays here because it implements Survivalist upgrade scopes and the exact C# shim contract; Modforge owns upgrade state and math.
#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_has_any(scope_id: i32, track: *const c_char) -> i32 {
    ffi::catch_or(0, || {
        let Some(scope) = scope(scope_id) else {
            return 0;
        };
        // SAFETY: C# keeps the supplied NUL-terminated track name live for this call.
        unsafe {
            ffi::with_utf8(
                track,
                |track| {
                    if UPGRADES.has_any(scope, track) { 1 } else { 0 }
                },
            )
        }
        .unwrap_or(0)
    })
}

/// Calculate the real material cost of the next upgrade level.
/// Stays here because it implements Survivalist upgrade scopes and the exact C# shim contract; Modforge owns upgrade state and math.
#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_cost(base_need: f64, factor: i32, next_level: i32) -> i32 {
    ffi::catch_or(0, || {
        modforge::upgrade::cost(base_need, i64::from(factor), i64::from(next_level)) as i32
    })
}

/// Calculate the skill required for the next upgrade level.
/// Stays here because it implements Survivalist upgrade scopes and the exact C# shim contract; Modforge owns upgrade state and math.
#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_skill(
    base: i32,
    levels_per_band: i32,
    next_level: i32,
) -> i32 {
    ffi::catch_or(base, || {
        modforge::upgrade::skill_requirement(
            i64::from(base),
            i64::from(levels_per_band),
            i64::from(next_level),
        ) as i32
    })
}

/// Calculate the diminishing benefit supplied by current upgrade levels.
/// Stays here because it implements Survivalist upgrade scopes and the exact C# shim contract; Modforge owns upgrade state and math.
#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_curve(level: i32, base_step: f32, decay: f32) -> f32 {
    ffi::catch_or(0.0, || {
        modforge::upgrade::diminishing_bonus(i64::from(level), base_step, decay)
    })
}

/// Return structure upgrade totals to the C# status surface.
/// Stays here because it implements Survivalist upgrade scopes and the exact C# shim contract; Modforge owns upgrade state and math.
#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_status() -> *mut c_char {
    ffi::catch_or(std::ptr::null_mut(), || {
        let status = UPGRADES.status(PROPS);
        let report = json!({
            "structures_upgraded": status.entities_upgraded,
            "levels_total": status.levels_total,
            "levels_per_track": status.levels_per_track,
            "seed": UPGRADES.slot().unwrap_or(0),
        })
        .to_string();
        ffi::string_into_raw(report)
    })
}

/// Release a status string previously returned to C#.
/// Stays here because its exported name is part of Survivalist's C# shim contract; Unityforge owns the returned string allocation.
#[unsafe(no_mangle)]
pub extern "C" fn survivalist_upgrade_string_free(ptr: *mut c_char) {
    ffi::catch_or((), || {
        // SAFETY: C# returns each non-null pointer from
        // survivalist_upgrade_status exactly once.
        unsafe { ffi::string_free(ptr) };
    });
}

/// Expose this system status and controls through the mod control endpoint.
/// Stays here because it implements Survivalist upgrade scopes and the exact C# shim contract; Modforge owns upgrade state and math.
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
/// Stays here because it implements Survivalist upgrade scopes and the exact C# shim contract; Modforge owns upgrade state and math.
fn parse_report(v: Json) -> Result<Json, String> {
    match v {
        Json::String(s) => {
            serde_json::from_str(&s).map_err(|e| format!("bad probe report json: {e}"))
        }
        other => Ok(other),
    }
}

/// Set and read one real structure upgrade through the game for live verification.
/// Stays here because it implements Survivalist upgrade scopes and the exact C# shim contract; Modforge owns upgrade state and math.
fn upgrade_probe(args: &Json) -> Result<Json, String> {
    let type_name = args
        .get("type")
        .and_then(Json::as_str)
        .ok_or("missing arg 'type' (prop prototype name, e.g. WoodenChest)")?
        .to_string();
    let level = args.get("level").and_then(Json::as_i64).unwrap_or(1);
    MAIN_QUEUE.run_result(
        "upgrade_probe",
        std::time::Duration::from_secs(5),
        move || {
            let v = mono::invoke_static(
                "SettlementUpgrades",
                "UpgradeProbe",
                &json!([type_name, level]),
            )?;
            parse_report(v)
        },
    )
}

/// Report structure and settlement upgrade effects from the C# shim.
/// Stays here because it implements Survivalist upgrade scopes and the exact C# shim contract; Modforge owns upgrade state and math.
fn upgrade_status(_args: &Json) -> Result<Json, String> {
    MAIN_QUEUE.run_result("upgrade_status", std::time::Duration::from_secs(5), || {
        let v = mono::invoke_static("SettlementUpgrades", "Status", &json!([]))?;
        parse_report(v)
    })
}
