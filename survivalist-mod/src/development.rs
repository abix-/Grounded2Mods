//! Structure growth (docs/faction-war.md "Town growth", the
//! structures half; annex model).
//!
//! Design (operator-locked): settlements expand ONE FENCED AREA
//! at a time. All the EXECUTION is the game's own: appending a
//! `ConstructionRecord` makes a settlement's Builder pick it up,
//! resolve the recipe, and build a real construction site from
//! hauled ingredients (Community.cs:6326; no cheating). So the
//! development brain is a PLANNER over `AddConstructionRecord`.
//!
//! This module lands in verifiable increments, not one blind
//! planner:
//! - `dev_status`: per-settlement base rect, buildable prototype
//!   availability, construction queues. Observability.
//! - `dev_place {community, prototype, dx, dy, orientation?}`:
//!   append ONE construction record at a tile offset from the
//!   base centre, through `AddConstructionRecord`. The live probe
//!   that proves append -> real build -> real material
//!   consumption before the annex geometry planner is written.
//!
//! Uses the game-struct marshalling (TerrainCoord as
//! `{"x":..,"y":..}`) added to the shim alongside this module.

use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::mono::MonoObject;

use crate::common::{
    ctype, display_name, for_each_community, handle_of, list_len, on_main_thread, own,
};

/// Perimeter/wall + gate + interior prototypes worth reporting on
/// (worldgen builds camps from these; recipes are story data, so
/// availability is probed live, not assumed).
const REPORTED_PROTOS: &[&str] = &[
    "WoodFence",
    "WireFence",
    "ConcreteWall",
    "WoodGate",
    "WireGate",
    "Shack",
    "Tent",
];

pub fn register_ops() {
    OP_REGISTRY.register_many([
        OpDef::new(
            "dev_status",
            "Per-settlement base rect, buildable-prototype availability (has a recipe?), and construction queues. The structure-growth observability surface.",
            "{}",
            dev_status,
        ),
        OpDef::new(
            "dev_place",
            "Append ONE ConstructionRecord to a named settlement at a tile offset from its base centre, through the game's own AddConstructionRecord. Probe: a member should pick it up and build it from real materials.",
            "{community: str, prototype: str, dx: int, dy: int, orientation?: str}",
            dev_place,
        ),
    ]);
}

fn game_impl() -> Result<MonoObject, String> {
    use unityforge::mono::MonoType;
    MonoType::find("GameImpl")
        .and_then(|t| t.singleton_instance())
        .ok_or_else(|| "GameImpl.Instance not found (no game loaded?)".to_string())
}

/// Resolve a PropPrototype by name and confirm it has a recipe
/// (a prototype with no recipe can never be built, so growth must
/// not queue it). Returns (proto_handle, has_recipe).
fn resolve_buildable(gi: &MonoObject, name: &str) -> Result<Option<(i32, bool)>, String> {
    let proto = gi.invoke("FindPropPrototypeByName", &json!([name]))?;
    let Some(ph) = handle_of(&proto) else {
        return Ok(None);
    };
    // FindRecipeByProduct(propProto, null) -> Recipe or null.
    let recipe = gi.invoke("FindRecipeByProduct", &json!([{ "handle": ph }, null]))?;
    let has_recipe = handle_of(&recipe).is_some();
    Ok(Some((ph, has_recipe)))
}

fn base_centre(com: &MonoObject) -> Option<(i32, i32)> {
    let rect = com.read_field("BaseRect").ok()?;
    let o = rect.as_object()?;
    let min = o.get("min")?.as_object()?;
    let max = o.get("max")?.as_object()?;
    let g = |m: &serde_json::Map<String, Json>, k: &str| m.get(k).and_then(Json::as_i64);
    let (minx, miny) = (g(min, "x")?, g(min, "y")?);
    let (maxx, maxy) = (g(max, "x")?, g(max, "y")?);
    Some((((minx + maxx) / 2) as i32, ((miny + maxy) / 2) as i32))
}

fn dev_status(_args: &Json) -> Result<Json, String> {
    on_main_thread(|| {
        let gi = game_impl()?;
        // Probe prototype availability once (global, not per community).
        let mut protos = Vec::new();
        for name in REPORTED_PROTOS {
            match resolve_buildable(&gi, name)? {
                Some((_, has_recipe)) => {
                    protos.push(json!({"name": name, "exists": true, "buildable": has_recipe}))
                }
                None => protos.push(json!({"name": name, "exists": false, "buildable": false})),
            }
        }
        let mut settlements = Vec::new();
        for_each_community(|com| {
            let t = ctype(&com);
            if t != "Normal" && t != "Looter" {
                return Ok(true);
            }
            if com.invoke("IsAISettlement", &json!([]))? != json!(true) {
                return Ok(true);
            }
            let members = com
                .invoke("GetLivingNonZombieMemberCount", &json!([]))?
                .as_i64()
                .unwrap_or(0);
            if members == 0 {
                return Ok(true);
            }
            settlements.push(json!({
                "name": display_name(&com),
                "type": t,
                "members": members,
                "base_rect": com.read_field("BaseRect").unwrap_or(Json::Null),
                "base_centre": base_centre(&com).map(|(x,y)| json!({"x":x,"y":y})).unwrap_or(Json::Null),
                "rebuild_queue": list_len(&com, "ConstructionRecords"),
                "repair_queue": list_len(&com, "NeedsRepair"),
                "building_now": list_len(&com, "UnderConstructionBuildings"),
            }));
            Ok(true)
        })?;
        Ok(json!({"prototypes": protos, "settlements": settlements}))
    })
}

fn dev_place(args: &Json) -> Result<Json, String> {
    let community = args
        .get("community")
        .and_then(Json::as_str)
        .ok_or("missing arg 'community' (settlement display name)")?
        .to_string();
    let prototype = args
        .get("prototype")
        .and_then(Json::as_str)
        .ok_or("missing arg 'prototype' (e.g. Shack, WoodFence)")?
        .to_string();
    let dx = args.get("dx").and_then(Json::as_i64).ok_or("missing arg 'dx' (int)")? as i32;
    let dy = args.get("dy").and_then(Json::as_i64).ok_or("missing arg 'dy' (int)")? as i32;
    let orientation = args
        .get("orientation")
        .and_then(Json::as_str)
        .unwrap_or("Deg0")
        .to_string();

    on_main_thread(move || {
        let gi = game_impl()?;
        let (proto_h, has_recipe) = resolve_buildable(&gi, &prototype)?
            .ok_or(format!("prototype '{prototype}' not found"))?;
        if !has_recipe {
            return Err(format!(
                "prototype '{prototype}' has no recipe; the game cannot build it"
            ));
        }

        let mut target: Option<i32> = None;
        for_each_community(|com| {
            if display_name(&com).eq_ignore_ascii_case(&community) && target.is_none() {
                target = Some(com.handle().0);
                std::mem::forget(com);
                return Ok(false);
            }
            Ok(true)
        })?;
        let com = own(target.ok_or(format!("settlement '{community}' not found"))?);

        let (cx, cy) = base_centre(&com).ok_or("settlement has no base rect")?;
        let (tx, ty) = (cx + dx, cy + dy);

        // AddConstructionRecord(PropPrototype, TerrainCoord,
        // OrientationType). TerrainCoord passes as a struct object;
        // OrientationType as its enum name.
        com.invoke(
            "AddConstructionRecord",
            &json!([
                { "handle": proto_h },
                { "x": tx, "y": ty },
                orientation,
            ]),
        )?;

        let queue = list_len(&com, "ConstructionRecords");
        unityforge::mono::log(
            unityforge::mono::LogLevel::Info,
            &format!(
                "survivalist-mod: dev -- queued {prototype} at ({tx},{ty}) for {} (rebuild queue now {queue})",
                display_name(&com)
            ),
        );
        Ok(json!({
            "queued": true,
            "prototype": prototype,
            "tile": {"x": tx, "y": ty},
            "community": display_name(&com),
            "rebuild_queue": queue,
        }))
    })
}
