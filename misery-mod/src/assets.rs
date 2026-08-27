//! Control-plane access to the asset registry.
//!
//! The registry itself is engine machinery and lives in
//! `ueforge::assets`: every asset the game ships, loaded or not,
//! plus loading one on demand. This module is only the two ops
//! that expose it over HTTP, and the game-thread routing they
//! need.
//!
//! That routing is the part worth keeping local: querying the
//! registry and streaming an asset both enter the engine, so both
//! go through this mod's game-thread queue. Called straight from
//! the HTTP worker they return null (research.md 26.1).

use std::time::Duration;

use ueforge::assets::{self, AssetEntry};

/// Long enough for a cold registry query over tens of thousands
/// of assets, and for a blocking asset load.
const ENGINE_TIMEOUT: Duration = Duration::from_secs(20);

/// Lists MISERY assets for players and mod authors through the debug API.
/// Stays here because it binds Ueforge's registry to this mod's game-thread queue and controls.
fn inventory_op(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let class = args
        .get("class")
        .and_then(|v| v.as_str())
        .unwrap_or("StaticMesh")
        .to_string();
    let filter = args
        .get("contains")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let class_for_job = class.clone();
    let rows = crate::dispatch::DRAIN.queue().enqueue(
        move || {
            let list: Vec<AssetEntry> = assets::assets_of_class(&class_for_job)?;
            let total = list.len();
            let rows: Vec<serde_json::Value> = list
                .into_iter()
                .filter(|a| {
                    filter.is_empty() || a.package.contains(&filter) || a.name.contains(&filter)
                })
                .map(|a| {
                    serde_json::json!({
                        "package": a.package,
                        "name": a.name,
                        "package_fname": a.package_fname,
                        "asset_fname": a.name_fname,
                    })
                })
                .collect();
            Ok(serde_json::json!({ "total": total, "assets": rows }))
        },
        ENGINE_TIMEOUT,
    )?;

    Ok(serde_json::json!({
        "class": class,
        "total": rows["total"],
        "returned": rows["assets"].as_array().map(|a| a.len()).unwrap_or(0),
        "assets": rows["assets"],
    }))
}

/// Loads one requested MISERY asset so it can be inspected or used in the running game.
/// Stays here because it binds Ueforge's loader to this mod's queue and debug operation.
fn load_op(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    let pkg = args
        .get("package_fname")
        .and_then(|v| v.as_u64())
        .ok_or("need {package_fname: u64}")?;
    let asset = args
        .get("asset_fname")
        .and_then(|v| v.as_u64())
        .ok_or("need {asset_fname: u64}")?;
    crate::dispatch::DRAIN.queue().enqueue(
        move || {
            let addr = assets::load_asset(pkg, asset)?;
            Ok(serde_json::json!({
                "loaded": addr != 0,
                "address": format!("{addr:#x}"),
            }))
        },
        ENGINE_TIMEOUT,
    )
}

/// Adds the MISERY asset commands to the mod's debug API.
/// Stays here because shared asset discovery and loading already live in Ueforge.
pub fn register_ops() {
    ueforge::ops::OP_REGISTRY.register_many([
        ueforge::ops::OpDef::new(
            "asset_inventory",
            "Every asset of a class the game ships, loaded or not",
            "{class?: str, contains?: str}",
            inventory_op,
        ),
        ueforge::ops::OpDef::new(
            "load_asset",
            "Pull an asset into memory by its package and asset FNames",
            "{package_fname: u64, asset_fname: u64}",
            load_op,
        ),
    ]);
}
