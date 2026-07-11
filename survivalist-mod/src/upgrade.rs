//! Settlement upgrades: the Rust observability surface
//! (docs/plans/2026-07-11-settlement-upgrades.md). The upgrade
//! state and the effect patches live in the C# shim
//! (cs-shim-survivalist/Upgrades.cs), which owns the seed-keyed
//! sidecar because its patches run inside the game's hot stat
//! reads; these ops drive its probe entries over invoke_static.

use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::mono;

use crate::common::on_main_thread;

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
