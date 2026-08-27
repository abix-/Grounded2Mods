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

use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::mono::{self, LogLevel, MonoObject};

use crate::common::{ctype, display_name, for_each_community, handle_of, on_main_thread, own};

/// Seconds between annex-planning scans (real time).
const ANNEX_SCAN_PERIOD_SECS: f32 = 120.0;

/// How deep (tiles) an annex extends from the existing base edge.
const ANNEX_DEPTH: i64 = 6;

/// A settlement only expands when it can feed more people.
const ANNEX_MIN_NUTRITION: f64 = 1.0;

/// Reject an annex side when more than this fraction of its fence
/// tiles sit on impassable ground.
const ANNEX_MAX_BLOCKED: f32 = 0.25;

/// The passability flags worldgen itself uses when validating camp
/// ground (GameTerrain.cs camp generation).
const IMPASSABLE_FLAGS: i64 = 2051;

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

/// Expose this system status and controls through the mod control endpoint.
/// Stays here because it applies Survivalist's build recipes, camp rules, terrain fields, and construction calls.
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

/// Open the game's story and prototype catalog.
/// Stays here because it applies Survivalist's build recipes, camp rules, terrain fields, and construction calls.
fn game_impl() -> Result<MonoObject, String> {
    use unityforge::mono::MonoType;
    MonoType::find("GameImpl")
        .and_then(|t| t.singleton_instance())
        .ok_or_else(|| "GameImpl.Instance not found (no game loaded?)".to_string())
}

/// Resolve a PropPrototype by name and confirm it has a recipe
/// (a prototype with no recipe can never be built, so growth must
/// not queue it). Returns (proto_handle, has_recipe).
/// Stays here because it applies Survivalist's build recipes, camp rules, terrain fields, and construction calls.
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

/// Find the center tile of a camp's claimed ground.
/// Stays here because it applies Survivalist's build recipes, camp rules, terrain fields, and construction calls.
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

/// Report which camps can expand and what they are already building.
/// Stays here because it applies Survivalist's build recipes, camp rules, terrain fields, and construction calls.
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
                "rebuild_queue": com.field_list_len("ConstructionRecords"),
                "repair_queue": com.field_list_len("NeedsRepair"),
                "building_now": com.field_list_len("UnderConstructionBuildings"),
            }));
            Ok(true)
        })?;
        Ok(json!({"prototypes": protos, "settlements": settlements}))
    })
}

// ---- the annex planner ------------------------------------------------------

/// f32 bits of the last scan's `now`; on_tick is main-thread only.
static LAST_ANNEX_SCAN_BITS: AtomicU32 = AtomicU32::new(0);

/// Advance this system when its scheduled game update is due.
/// Stays here because it applies Survivalist's build recipes, camp rules, terrain fields, and construction calls.
pub fn tick(now: f32) {
    let last = f32::from_bits(LAST_ANNEX_SCAN_BITS.load(Ordering::Relaxed));
    if now - last < ANNEX_SCAN_PERIOD_SECS {
        return;
    }
    LAST_ANNEX_SCAN_BITS.store(now.to_bits(), Ordering::Relaxed);
    if let Err(e) = annex_scan() {
        if !e.contains("not found") {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: dev annex scan failed: {e}"),
            );
        }
    }
}

struct Rect {
    minx: i64,
    miny: i64,
    maxx: i64,
    maxy: i64,
}

/// Read a camp's claimed rectangle into the planner format.
/// Stays here because it applies Survivalist's build recipes, camp rules, terrain fields, and construction calls.
fn base_rect(com: &MonoObject) -> Option<Rect> {
    let rect = com.read_field("BaseRect").ok()?;
    let o = rect.as_object()?;
    let min = o.get("min")?.as_object()?;
    let max = o.get("max")?.as_object()?;
    let g = |m: &serde_json::Map<String, Json>, k: &str| m.get(k).and_then(Json::as_i64);
    let r = Rect {
        minx: g(min, "x")?,
        miny: g(min, "y")?,
        maxx: g(max, "x")?,
        maxy: g(max, "y")?,
    };
    if r.minx < 0 || r.maxx <= r.minx || r.maxy <= r.miny {
        return None; // TerrainRect.Invalid or degenerate
    }
    Some(r)
}

/// Open the loaded map terrain used to reject blocked construction.
/// Stays here because it applies Survivalist's build recipes, camp rules, terrain fields, and construction calls.
fn game_terrain() -> Result<MonoObject, String> {
    use unityforge::mono::MonoType;
    MonoType::find("GameTerrain")
        .and_then(|t| t.singleton_instance())
        .ok_or_else(|| "GameTerrain.Instance not found".to_string())
}

/// Ask the game whether settlers can build on a tile.
/// Stays here because it applies Survivalist's build recipes, camp rules, terrain fields, and construction calls.
fn is_impassable(terrain: &MonoObject, x: i64, y: i64) -> bool {
    terrain
        .invoke("IsImpassable", &json!([x, y, IMPASSABLE_FLAGS, null, null]))
        .map(|v| v == json!(true))
        .unwrap_or(true) // unreadable ground counts as blocked
}

/// The annex's fence line: the three exposed sides of a strip
/// attached to one side of the base (the shared side keeps the
/// base's existing wall; the annex gets its own gate in the
/// middle of its outer edge).
struct AnnexPlan {
    side: &'static str,
    fence_tiles: Vec<(i64, i64)>,
    gate_tile: (i64, i64),
    hut_tile: (i64, i64),
    new_rect: Rect,
}

/// Choose a buildable fenced extension beside a crowded camp.
/// Extraction candidate: Modforge should own engine-independent rectangular annex planning; Survivalist should supply terrain passability and construction policy.
fn plan_annex(base: &Rect, terrain: &MonoObject) -> Option<AnnexPlan> {
    // Try east, south, west, north.
    let sides: [(&str, i64, i64); 4] = [
        ("east", 1, 0),
        ("south", 0, 1),
        ("west", -1, 0),
        ("north", 0, -1),
    ];
    for (side, sx, sy) in sides {
        // The annex strip rectangle.
        let (aminx, aminy, amaxx, amaxy) = match (sx, sy) {
            (1, 0) => (base.maxx + 1, base.miny, base.maxx + ANNEX_DEPTH, base.maxy),
            (-1, 0) => (base.minx - ANNEX_DEPTH, base.miny, base.minx - 1, base.maxy),
            (0, 1) => (base.minx, base.maxy + 1, base.maxx, base.maxy + ANNEX_DEPTH),
            _ => (base.minx, base.miny - ANNEX_DEPTH, base.maxx, base.miny - 1),
        };
        if aminx < 1 || aminy < 1 {
            continue;
        }
        // Fence tiles: the annex perimeter minus the side shared
        // with the base.
        let mut fence: Vec<(i64, i64)> = Vec::new();
        for x in aminx..=amaxx {
            if sy != 1 {
                fence.push((x, aminy)); // north edge (skip if shared)
            }
            if sy != -1 {
                fence.push((x, amaxy)); // south edge
            }
        }
        for y in (aminy + 1)..amaxy {
            if sx != 1 {
                fence.push((aminx, y)); // west edge
            }
            if sx != -1 {
                fence.push((amaxx, y)); // east edge
            }
        }
        let blocked = fence
            .iter()
            .filter(|(x, y)| is_impassable(terrain, *x, *y))
            .count();
        if (blocked as f32) > (fence.len() as f32) * ANNEX_MAX_BLOCKED {
            continue;
        }
        // Gate: the middle of the outer edge.
        let gate = match (sx, sy) {
            (1, 0) => (amaxx, (aminy + amaxy) / 2),
            (-1, 0) => (aminx, (aminy + amaxy) / 2),
            (0, 1) => ((aminx + amaxx) / 2, amaxy),
            _ => ((aminx + amaxx) / 2, aminy),
        };
        let hut = ((aminx + amaxx) / 2, (aminy + amaxy) / 2);
        if is_impassable(terrain, hut.0, hut.1) {
            continue;
        }
        let fence_tiles: Vec<(i64, i64)> = fence
            .into_iter()
            .filter(|t| *t != gate && !is_impassable(terrain, t.0, t.1))
            .collect();
        return Some(AnnexPlan {
            side,
            fence_tiles,
            gate_tile: gate,
            hut_tile: hut,
            new_rect: Rect {
                minx: base.minx.min(aminx),
                miny: base.miny.min(aminy),
                maxx: base.maxx.max(amaxx),
                maxy: base.maxy.max(amaxy),
            },
        });
    }
    None
}

/// Find one healthy, crowded camp ready to plan its next annex.
/// Stays here because it applies Survivalist's build recipes, camp rules, terrain fields, and construction calls.
fn annex_scan() -> Result<(), String> {
    let gi = game_impl()?;
    let terrain = game_terrain()?;
    let mut planned = false;
    for_each_community(|com| {
        if planned {
            return Ok(false);
        }
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
        let beds = com
            .invoke("GetAccommodation", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        if members < beds {
            return Ok(true); // no growth pressure yet
        }
        if com
            .invoke("CalcCommunityNutritionLevel", &json!([0.0]))?
            .as_f64()
            .unwrap_or(0.0)
            < ANNEX_MIN_NUTRITION
        {
            return Ok(true); // cannot feed more people
        }
        if com.field_list_len("ConstructionRecords") > 0
            || com.field_list_len("UnderConstructionBuildings") > 0
        {
            return Ok(true); // already building something
        }
        let Some(base) = base_rect(&com) else {
            return Ok(true);
        };
        let Some(plan) = plan_annex(&base, &terrain) else {
            return Ok(true);
        };

        // Per-type wall doctrine: Normal builds wood, Looters wire.
        let (fence_name, gate_name) = if t == "Looter" {
            ("WireFence", "WireGate")
        } else {
            ("WoodFence", "WoodGate")
        };
        let Some((fence_proto, true)) = resolve_buildable(&gi, fence_name)? else {
            return Ok(true);
        };
        let Some((gate_proto, true)) = resolve_buildable(&gi, gate_name)? else {
            return Ok(true);
        };
        let Some((hut_proto, true)) = resolve_buildable(&gi, "Shack")? else {
            return Ok(true);
        };

        // Fence FIRST (the area is not safe until enclosed), then
        // the gate, then the interior structure: the game builds
        // records roughly in list order.
        for (x, y) in &plan.fence_tiles {
            com.invoke(
                "AddConstructionRecord",
                &json!([{ "handle": fence_proto }, { "x": x, "y": y }, "Deg0"]),
            )?;
        }
        com.invoke(
            "AddConstructionRecord",
            &json!([
                { "handle": gate_proto },
                { "x": plan.gate_tile.0, "y": plan.gate_tile.1 },
                "Deg0",
            ]),
        )?;
        com.invoke(
            "AddConstructionRecord",
            &json!([
                { "handle": hut_proto },
                { "x": plan.hut_tile.0, "y": plan.hut_tile.1 },
                "Deg0",
            ]),
        )?;

        // Adopt the annex: the settlement claims the ground it is
        // about to enclose.
        com.write_field(
            "BaseRect",
            &json!({
                "min": {"x": plan.new_rect.minx, "y": plan.new_rect.miny},
                "max": {"x": plan.new_rect.maxx, "y": plan.new_rect.maxy},
            }),
        )?;

        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: dev -- {} plans an annex to the {}: {} {} posts, a {}, and a shack (beds full at {}/{})",
                display_name(&com),
                plan.side,
                plan.fence_tiles.len(),
                fence_name,
                gate_name,
                members,
                beds
            ),
        );
        planned = true; // one annex per scan, map-wide: organic pace
        Ok(false)
    })?;
    Ok(())
}

/// Queue one real construction job near a named camp for live verification.
/// Stays here because it applies Survivalist's build recipes, camp rules, terrain fields, and construction calls.
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
    let dx = args
        .get("dx")
        .and_then(Json::as_i64)
        .ok_or("missing arg 'dx' (int)")? as i32;
    let dy = args
        .get("dy")
        .and_then(Json::as_i64)
        .ok_or("missing arg 'dy' (int)")? as i32;
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

        let queue = com.field_list_len("ConstructionRecords");
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
