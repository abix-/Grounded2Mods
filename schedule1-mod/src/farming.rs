//! Persistent zone garrisons + the influence war (approved plan
//! 2026-08-08, C:/Users/Abix/.claude/plans/
//! sequential-hatching-hopper.md).
//!
//! Every cartel region keeps its garrison standing LIVE across
//! the whole map at all times; the player travels to them.
//! Garrison deaths cost the cartel influence there
//! (ChangeInfluence through the game's own machinery), lower
//! influence shrinks the garrison target, and a region ground
//! to zero influence with no live force flips to the PLAYER
//! (takeover trigger; holding machinery = later slices).
//!
//! forces are keyed by faction so player garrisons and
//! goon-vs-goon war drop in without reshaping. Mobs roll hidden
//! MODIFIER TYPES (Diablo champion model); every roll, weight,
//! and radius below is spoiler-firewalled: the operator reads
//! shapes, never specifics.
//!
//! Anchors, all live-proven: Map.Regions (bounds polygon +
//! delivery-location anchors), CartelInfluence.GetInfluence,
//! GoonPool.SpawnGoon, AttackEntity, goon Health/Movement
//! writes, SetDestination.

use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::json;

use unityforge::mono::{self, LogLevel, MonoType};

use crate::loot::{handle_of, own, parse_vec3};

#[derive(Clone, Copy, PartialEq)]
enum Faction {
    Cartel,
    Player,
    None,
}

struct Region {
    name: String,
    rank: i64,
    posts: Vec<(f64, f64, f64)>,
    owner: Faction,
    casualties: u32,
    next_reinforce: Option<Instant>,
}

struct Mob {
    region_idx: usize,
    faction: Faction,
    goon_h: i64,
    npc_ptr: i64,
    post: (f64, f64, f64),
    aggro_radius: f64,
    aggroed: bool,
    label: String,
    xp_mult: f32,
    loot_mult: f32,
    last_order: Instant,
}

static REGIONS: Mutex<Vec<Region>> = Mutex::new(Vec::new());
static FORCES: Mutex<Vec<Mob>> = Mutex::new(Vec::new());
static LAST_PASS: Mutex<Option<Instant>> = Mutex::new(None);
static KILLS_BY_REGION: Mutex<Vec<(String, u32)>> = Mutex::new(Vec::new());

/// Spoiler firewall: pacing, population, radii, war math.
const PASS_EVERY: Duration = Duration::from_secs(4);
const HOLD_ORDER_EVERY: Duration = Duration::from_secs(12);
const ANCHOR_JITTER: f64 = 12.0;
const STRAY_LIMIT: f64 = 30.0;
const TOTAL_LIVE_CAP: usize = 24;
const SPAWNS_PER_PASS: usize = 2;
const REINFORCE_SECS: (u64, u64) = (90, 240);
const INFLUENCE_PER_KILL: (f64, f64) = (0.03, 0.06);

pub fn tick() {
    if !crate::skills::SETTLED.load(std::sync::atomic::Ordering::Relaxed) {
        FORCES.lock().clear();
        REGIONS.lock().clear();
        return;
    }
    {
        let now = Instant::now();
        let mut last = LAST_PASS.lock();
        if last.is_some_and(|t| now.duration_since(t) < PASS_EVERY) {
            return;
        }
        *last = Some(now);
    }
    if REGIONS.lock().is_empty() {
        match load_regions() {
            Ok(n) => mono::log(
                LogLevel::Info,
                &format!("schedule1-mod: {n} regions mapped; the cartel is taking its posts"),
            ),
            Err(e) => {
                mono::log(LogLevel::Warn, &format!("schedule1-mod: region load failed: {e}"));
                return;
            }
        }
    }
    if let Err(e) = war_pass() {
        mono::log(LogLevel::Warn, &format!("schedule1-mod: war pass failed: {e}"));
    }
}

/// The mob's rolled multipliers, consumed on down. Also the
/// influence engine: a garrison death costs the owner influence
/// in that zone, and may trigger the takeover.
pub fn on_mob_down(npc_ptr: i64) -> Option<(f32, f32, String)> {
    let mut forces = FORCES.lock();
    let i = forces.iter().position(|m| m.npc_ptr == npc_ptr)?;
    let m = forces.remove(i);
    drop(own(m.goon_h));
    let live_here = forces.iter().filter(|x| x.region_idx == m.region_idx).count();
    drop(forces);

    let region_name = {
        let mut regions = REGIONS.lock();
        match regions.get_mut(m.region_idx) {
            Some(r) => {
                r.casualties += 1;
                if r.next_reinforce.is_none() {
                    r.next_reinforce = Some(Instant::now() + roll_reinforce());
                }
                r.name.clone()
            }
            None => String::new(),
        }
    };
    {
        let mut kills = KILLS_BY_REGION.lock();
        match kills.iter_mut().find(|(n, _)| *n == region_name) {
            Some((_, c)) => *c += 1,
            None => kills.push((region_name.clone(), 1)),
        }
    }

    // The war ledger: this death costs the cartel its grip.
    if m.faction == Faction::Cartel {
        let delta = INFLUENCE_PER_KILL.0
            + fastrand::f64() * (INFLUENCE_PER_KILL.1 - INFLUENCE_PER_KILL.0);
        match change_influence(m.region_idx, -delta) {
            Ok(now_at) => {
                mono::log(
                    LogLevel::Info,
                    &format!("schedule1-mod: the cartel's grip on {region_name} weakens"),
                );
                if now_at <= 0.005 && live_here == 0 {
                    if let Some(r) = REGIONS.lock().get_mut(m.region_idx) {
                        r.owner = Faction::Player;
                        r.next_reinforce = None;
                    }
                    mono::log(
                        LogLevel::Info,
                        &format!("schedule1-mod: ==== {region_name} IS YOURS ===="),
                    );
                }
            }
            Err(e) => mono::log(
                LogLevel::Warn,
                &format!("schedule1-mod: influence change failed: {e}"),
            ),
        }
    }
    Some((m.xp_mult, m.loot_mult, format!("{} ({region_name})", m.label)))
}

/// Kill tally per region since load.
#[allow(dead_code)]
pub fn kills_by_region() -> Vec<(String, u32)> {
    KILLS_BY_REGION.lock().clone()
}

fn roll_reinforce() -> Duration {
    Duration::from_secs(fastrand::u64(REINFORCE_SECS.0..=REINFORCE_SECS.1))
}

// ---- game plumbing ---------------------------------------------------

fn cartel_influence_instance() -> Result<unityforge::mono::MonoObject, String> {
    // Not a Singleton<T>; the one live instance comes via a walk.
    let ty = MonoType::find("Il2CppScheduleOne.Cartel.CartelInfluence")
        .ok_or("CartelInfluence type not found")?;
    let walked = ty.walk(false)?;
    let ih = walked
        .as_array()
        .and_then(|a| a.first())
        .and_then(|i| i["handle"].as_i64())
        .ok_or("no live CartelInfluence")?;
    Ok(own(ih))
}

fn get_influence(region_idx: usize) -> Result<f64, String> {
    let inst = cartel_influence_instance()?;
    Ok(inst
        .invoke("GetInfluence", &json!([region_idx as i64]))?
        .as_f64()
        .unwrap_or(0.0))
}

/// Move a region's influence through the game's own machinery
/// and report where it landed.
fn change_influence(region_idx: usize, delta: f64) -> Result<f64, String> {
    let inst = cartel_influence_instance()?;
    inst.invoke("ChangeInfluence", &json!([region_idx as i64, delta]))?;
    drop(inst);
    get_influence(region_idx)
}

fn load_regions() -> Result<usize, String> {
    let map_ty = MonoType::find("Il2CppScheduleOne.Map.Map").ok_or("Map type not found")?;
    let map = map_ty.singleton_instance().ok_or("no Map singleton")?;
    let regions_v = map.read_field("Regions")?;
    let rh = handle_of(&regions_v).ok_or("Regions carried no handle")?;
    let regions_arr = own(rh);
    let n = regions_arr
        .invoke("get_Length", &json!([]))?
        .as_i64()
        .ok_or("Regions length unreadable")?;
    let mut out = Vec::new();
    for i in 0..n {
        let item = regions_arr.invoke("get_Item", &json!([i]))?;
        let Some(reg_h) = handle_of(&item) else { continue };
        let reg = own(reg_h);
        let name = reg
            .read_field("Name")
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("region {i}"));
        let rank = reg
            .read_field("RankRequirement")
            .ok()
            .and_then(|v| {
                v.get("str")
                    .and_then(serde_json::Value::as_str)
                    .map(String::from)
            })
            .and_then(|s| s.split(' ').next_back().and_then(|t| t.parse().ok()))
            .unwrap_or(0);

        // Posts: rolled ONCE per save from the anchor set.
        // (Region bounds polygons are readable too when a later
        // slice needs player-region detection; recipe in
        // tests/research_zones.rs.)
        let mut posts = Vec::new();
        if let Ok(dl) = reg.read_field("RegionDeliveryLocations") {
            if let Some(dlh) = handle_of(&dl) {
                let list = own(dlh);
                let count = list
                    .invoke("get_Count", &json!([]))
                    .ok()
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                for j in 0..count {
                    let Ok(e) = list.invoke("get_Item", &json!([j])) else { continue };
                    let Some(eh) = handle_of(&e) else { continue };
                    let loc = own(eh);
                    if let Ok(t) = loc.read_field("transform") {
                        if let Some(th) = handle_of(&t) {
                            let tr = own(th);
                            if let Some((x, y, z)) =
                                tr.invoke("get_position", &json!([])).ok().and_then(|p| parse_vec3(&p))
                            {
                                let jx = x + (fastrand::f64() - 0.5) * 2.0 * ANCHOR_JITTER;
                                let jz = z + (fastrand::f64() - 0.5) * 2.0 * ANCHOR_JITTER;
                                posts.push((jx, y, jz));
                            }
                        }
                    }
                }
            }
        }
        let influence = get_influence(i as usize).unwrap_or(0.0);
        out.push(Region {
            name,
            rank,
            posts,
            owner: if influence > 0.005 { Faction::Cartel } else { Faction::None },
            casualties: 0,
            next_reinforce: None,
        });
    }
    let count = out.len();
    *REGIONS.lock() = out;
    Ok(count)
}

/// A region's garrison target from its CURRENT influence (kills
/// compound: less influence, thinner garrison).
fn strength_target(influence: f64, rank: i64) -> usize {
    if influence <= 0.05 {
        0
    } else {
        1 + (influence * 3.0).round() as usize + (rank as usize) / 4
    }
}

/// One throttled pass of the standing war: fill garrisons up to
/// their influence-derived targets everywhere (a few spawns per
/// pass), run reinforcement timers, hold posts, aggro.
fn war_pass() -> Result<(), String> {
    // Player position (for aggro checks only; garrisons do not
    // care where the player is).
    let player_ty =
        MonoType::find("Il2CppScheduleOne.PlayerScripts.Player").ok_or("Player type not found")?;
    let players = player_ty.walk(false)?;
    let player_h = players
        .as_array()
        .and_then(|a| a.first())
        .and_then(|i| i["handle"].as_i64())
        .ok_or("no live Player")?;
    let player = own(player_h);
    let ppos = {
        let transform = player.read_field("transform")?;
        let th = handle_of(&transform).ok_or("no player transform")?;
        let t = own(th);
        parse_vec3(&t.invoke("get_position", &json!([]))?).ok_or("bad player position")?
    };

    // The cartel's supply is the vanilla goon pool (5 objects in
    // this game). Check it before spawning, and give the scarce
    // goons to the strongest zones first. Growing the pool is a
    // backlog research item.
    let mut supply = {
        let pool_ty = MonoType::find("Il2CppScheduleOne.Cartel.GoonPool")
            .ok_or("GoonPool type not found")?;
        let pools = pool_ty.walk(false)?;
        let pool_h = pools
            .as_array()
            .and_then(|a| a.first())
            .and_then(|i| i["handle"].as_i64())
            .ok_or("no live GoonPool")?;
        let pool = own(pool_h);
        pool.invoke("get_UnspawnedGoonCount", &json!([]))?
            .as_i64()
            .unwrap_or(0)
    };

    let now = Instant::now();
    let mut spawned = 0usize;
    // Every cartel region that is under strength and off its
    // reinforcement cooldown, strongest grip first.
    let mut wanting: Vec<(usize, f64, String, (f64, f64, f64))> = Vec::new();
    let region_count = REGIONS.lock().len();
    for idx in 0..region_count {
        let (name, rank, owner, post, ready) = {
            let mut regions = REGIONS.lock();
            let r = &mut regions[idx];
            if r.posts.is_empty() {
                continue;
            }
            let post = r.posts[fastrand::usize(0..r.posts.len())];
            // Reinforcement timer gates refills AFTER casualties;
            // the initial fill (no casualties yet) is immediate.
            let ready = match r.next_reinforce {
                None => true,
                Some(t) if now >= t => {
                    r.next_reinforce = None;
                    true
                }
                Some(_) => false,
            };
            (r.name.clone(), r.rank, r.owner, post, ready)
        };
        if owner != Faction::Cartel || !ready {
            continue;
        }
        let influence = get_influence(idx)?;
        let target = strength_target(influence, rank);
        let live = FORCES
            .lock()
            .iter()
            .filter(|m| m.region_idx == idx && m.faction == Faction::Cartel)
            .count();
        if live < target {
            wanting.push((idx, influence, name, post));
        }
    }
    wanting.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    for (idx, _, name, post) in wanting {
        if spawned >= SPAWNS_PER_PASS || supply <= 0 || FORCES.lock().len() >= TOTAL_LIVE_CAP {
            break;
        }
        match spawn_mob(idx, &name, post) {
            Ok(reinforcement) => {
                spawned += 1;
                supply -= 1;
                if reinforcement {
                    mono::log(
                        LogLevel::Info,
                        &format!("schedule1-mod: the cartel reinforces {name}"),
                    );
                }
            }
            Err(e) => {
                // Pool dry mid-pass is normal scarcity, not an
                // error worth spamming.
                if !e.contains("carried no handle") {
                    mono::log(
                        LogLevel::Warn,
                        &format!("schedule1-mod: spawn in {name} failed: {e}"),
                    );
                }
                break;
            }
        }
    }

    hold_posts(ppos.0, ppos.2, player_h);
    Ok(())
}

/// Keep stationed mobs at their posts; aggro when the player is
/// close. One order per mob per window.
fn hold_posts(px: f64, pz: f64, player_h: i64) {
    let now = Instant::now();
    let mut forces = FORCES.lock();
    for g in forces.iter_mut() {
        if g.aggroed || now.duration_since(g.last_order) < HOLD_ORDER_EVERY {
            continue;
        }
        g.last_order = now;
        let goon = own(g.goon_h);
        let mut gpos = None;
        if let Ok(t) = goon.read_field("transform") {
            if let Some(th) = handle_of(&t) {
                let tr = own(th);
                gpos = tr.invoke("get_position", &json!([])).ok().and_then(|p| parse_vec3(&p));
            }
        }
        std::mem::forget(goon); // the registry keeps the handle
        let Some((gx, _, gz)) = gpos else { continue };
        let d_player = ((gx - px).powi(2) + (gz - pz).powi(2)).sqrt();
        if d_player <= g.aggro_radius {
            let goon = own(g.goon_h);
            let r = goon.invoke("AttackEntity", &json!([{"$handle": player_h}, true]));
            std::mem::forget(goon);
            if r.is_ok() {
                g.aggroed = true;
                mono::log(LogLevel::Info, &format!("schedule1-mod: {} noticed you", g.label));
            }
            continue;
        }
        let d_post = ((gx - g.post.0).powi(2) + (gz - g.post.2).powi(2)).sqrt();
        if d_post > STRAY_LIMIT {
            let goon = own(g.goon_h);
            if let Ok(mv) = goon.read_field("Movement") {
                if let Some(mh) = handle_of(&mv) {
                    let m = own(mh);
                    let _ = m.invoke(
                        "SetDestination",
                        &json!([{"x": g.post.0, "y": g.post.1, "z": g.post.2}]),
                    );
                }
            }
            std::mem::forget(goon);
        }
    }
}

/// Spawn one cartel mob at a post. Returns true when it was a
/// reinforcement (the region already took casualties).
fn spawn_mob(region_idx: usize, region_name: &str, post: (f64, f64, f64)) -> Result<bool, String> {
    let pool_ty =
        MonoType::find("Il2CppScheduleOne.Cartel.GoonPool").ok_or("GoonPool type not found")?;
    let pools = pool_ty.walk(false)?;
    let pool_h = pools
        .as_array()
        .and_then(|a| a.first())
        .and_then(|i| i["handle"].as_i64())
        .ok_or("no live GoonPool")?;
    let pool = own(pool_h);
    let spawn = pool.invoke("SpawnGoon", &json!([{"x": post.0, "y": post.1, "z": post.2}]))?;
    let goon_h = handle_of(&spawn).ok_or("SpawnGoon carried no handle")?;
    let goon = own(goon_h);

    // Roll the mob's types.
    let roll = fastrand::f32();
    let affix_count = if roll < 0.45 {
        0
    } else if roll < 0.80 {
        1
    } else if roll < 0.95 {
        2
    } else {
        3
    };
    let mut names: Vec<&str> = Vec::new();
    let mut xp_mult = 1.0f32;
    let mut loot_mult = 1.0f32;
    let mut pool_types = ["tough", "swift", "veteran"];
    fastrand::shuffle(&mut pool_types);

    let health = goon.read_field("Health")?;
    let health_h = handle_of(&health).ok_or("goon Health carried no handle")?;
    let health_obj = own(health_h);
    let mut npc_ptr = None;
    if let Ok(npc) = health_obj.read_field("npc") {
        npc_ptr = npc.get("ptr").and_then(serde_json::Value::as_i64);
        if let Some(h) = handle_of(&npc) {
            drop(own(h));
        }
    }
    for ty_name in pool_types.iter().take(affix_count) {
        match *ty_name {
            "tough" => {
                let mult = 2.0 + fastrand::f32() * 2.0;
                let _ = health_obj.write_field("Health", &json!(100.0 * mult));
                xp_mult += 1.0;
                loot_mult += mult * 0.5;
                names.push("Tough");
            }
            "swift" => {
                if let Ok(mv) = goon.read_field("Movement") {
                    if let Some(mh) = handle_of(&mv) {
                        let m = own(mh);
                        let cur = m
                            .read_field("MoveSpeedMultiplier")
                            .ok()
                            .and_then(|v| v.as_f64())
                            .unwrap_or(1.0);
                        let boost = 1.3 + fastrand::f64() * 0.5;
                        let _ = m.write_field("MoveSpeedMultiplier", &json!(cur * boost));
                        xp_mult += 0.5;
                        loot_mult += 0.5;
                        names.push("Swift");
                    }
                }
            }
            "veteran" => {
                // Pays more, shows nothing. Mystery on purpose.
                xp_mult += 0.5;
                loot_mult += 1.5;
                names.push("Veteran");
            }
            _ => {}
        }
    }

    let label = if names.is_empty() {
        "a goon".to_string()
    } else {
        format!("a {} goon", names.join(" "))
    };
    let Some(ptr) = npc_ptr else {
        return Err("goon npc ptr unreadable".into());
    };
    let aggro_radius = 14.0 + fastrand::f64() * 12.0;
    let goon_h_kept = goon.handle().0 as i64;
    std::mem::forget(goon);
    let reinforcement = REGIONS
        .lock()
        .get(region_idx)
        .map(|r| r.casualties > 0)
        .unwrap_or(false);
    FORCES.lock().push(Mob {
        region_idx,
        faction: Faction::Cartel,
        goon_h: goon_h_kept,
        npc_ptr: ptr,
        post,
        aggro_radius,
        aggroed: false,
        label: label.clone(),
        xp_mult,
        loot_mult,
        last_order: Instant::now(),
    });
    if !reinforcement {
        mono::log(
            LogLevel::Info,
            &format!("schedule1-mod: {label} now holds {region_name}"),
        );
    }
    Ok(reinforcement)
}

/// Observability op. Counts, owners, and positions only:
/// rolled specifics stay behind the spoiler firewall.
pub fn register_ops() {
    modforge::ops::OP_REGISTRY.register(modforge::ops::OpDef::new(
        "farm_state",
        "The war map: per region owner, influence, strength target, live force, casualties; live mob positions. No rolled specifics (spoiler firewall).",
        "{}",
        |_args| {
            let snapshot: Vec<(i64, String, (f64, f64, f64), bool)> = {
                let regions = REGIONS.lock();
                FORCES
                    .lock()
                    .iter()
                    .map(|g| {
                        let region = regions
                            .get(g.region_idx)
                            .map(|r| r.name.clone())
                            .unwrap_or_else(|| "?".into());
                        (g.goon_h, region, g.post, g.aggroed)
                    })
                    .collect()
            };
            // Game-touching reads on the MAIN thread only.
            let per_region = unityforge::main_thread_queue::MAIN_QUEUE
                .run("farm_state_regions", std::time::Duration::from_secs(2), || {
                    let regions = REGIONS.lock();
                    let forces = FORCES.lock();
                    regions
                        .iter()
                        .enumerate()
                        .map(|(i, r)| {
                            let live =
                                forces.iter().filter(|g| g.region_idx == i).count();
                            let influence = get_influence(i).unwrap_or(-1.0);
                            json!({
                                "region": r.name,
                                "owner": match r.owner {
                                    Faction::Cartel => "cartel",
                                    Faction::Player => "player",
                                    Faction::None => "none",
                                },
                                "influence": influence,
                                "strength_target": strength_target(influence, r.rank),
                                "live": live,
                                "casualties": r.casualties,
                                "reinforcement_pending": r.next_reinforce.is_some(),
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mobs = unityforge::main_thread_queue::MAIN_QUEUE
                .run("farm_state_mobs", std::time::Duration::from_secs(2), move || {
                    snapshot
                        .into_iter()
                        .map(|(goon_h, region, post, aggroed)| {
                            let goon = own(goon_h);
                            let mut pos = None;
                            if let Ok(t) = goon.read_field("transform") {
                                if let Some(th) = handle_of(&t) {
                                    let tr = own(th);
                                    pos = tr
                                        .invoke("get_position", &json!([]))
                                        .ok()
                                        .and_then(|p| parse_vec3(&p));
                                }
                            }
                            std::mem::forget(goon);
                            json!({
                                "region": region,
                                "post": {"x": post.0, "y": post.1, "z": post.2},
                                "pos": pos.map(|(x, y, z)| json!({"x": x, "y": y, "z": z})),
                                "aggroed": aggroed,
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let kills: Vec<serde_json::Value> = KILLS_BY_REGION
                .lock()
                .iter()
                .map(|(n, c)| json!({"region": n, "kills": c}))
                .collect();
            Ok(json!({"regions": per_region, "kills": kills, "mobs": mobs}))
        },
    ));
}
