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

use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::json;

use modforge::client::parse_vec3;
use unityforge::mono::{self, LogLevel, MonoType};

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
    /// NpcFactory mint index (the C# side's handle).
    minted_index: i64,
    npc_ptr: i64,
    post: (f64, f64, f64),
    aggro_radius: f64,
    aggroed: bool,
    label: String,
    xp_mult: f32,
    loot_mult: f32,
    last_order: Instant,
    /// Rolled at spawn, applied once the S1API pipeline has
    /// materialized the NPC (a few seconds after minting).
    spawned_at: Instant,
    pending_toughness: Option<f32>,
    pending_weapon: Option<&'static str>,
    affixes_applied: bool,
}

/// How long after minting before affixes land (the S1API spawn
/// pipeline settles in 3-6s).
const AFFIX_DELAY: Duration = Duration::from_secs(8);

const FACTORY: &str = "Unityforge.Shim.Schedule1.NpcFactory";

/// The factory answers with a JSON STRING; parse it.
/// Stays here because the factory and its response contract are Schedule 1 shim details; Unityforge owns generic static invocation.
fn factory_call(method: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
    let v = mono::invoke_static(FACTORY, method, &args)?;
    let s = v.as_str().ok_or_else(|| format!("{method}: non-string factory result: {v}"))?;
    let parsed: serde_json::Value =
        serde_json::from_str(s).map_err(|e| format!("{method}: bad factory json: {e}"))?;
    if parsed["ok"].as_bool() != Some(true) {
        return Err(format!("{method}: {}", parsed["error"]));
    }
    Ok(parsed)
}

static REGIONS: Mutex<Vec<Region>> = Mutex::new(Vec::new());
static FORCES: Mutex<Vec<Mob>> = Mutex::new(Vec::new());
static LAST_PASS: Mutex<Option<Instant>> = Mutex::new(None);
static KILLS_BY_REGION: Mutex<Vec<(String, u32)>> = Mutex::new(Vec::new());

/// Spoiler firewall: pacing, population, radii, war math.
const PASS_EVERY: Duration = Duration::from_secs(4);
const HOLD_ORDER_EVERY: Duration = Duration::from_secs(12);
const ANCHOR_JITTER: f64 = 12.0;
const TOTAL_LIVE_CAP: usize = 24;
const SPAWNS_PER_PASS: usize = 2;
const REINFORCE_SECS: (u64, u64) = (90, 240);
const INFLUENCE_PER_KILL: (f64, f64) = (0.03, 0.06);

/// Verbose war logging (set false to quiet down once proven).
const WAR_VERBOSE: bool = true;

/// Writes an optional Schedule 1 war diagnostic with a consistent label.
/// Stays here because verbosity and message context are local presentation; Unityforge owns the log sink.
fn vlog(msg: &str) {
    if WAR_VERBOSE {
        mono::log(LogLevel::Info, &format!("schedule1-mod [war]: {msg}"));
    }
}

/// Advances Schedule 1's regional war at a throttled rate once the loaded save is safe.
/// Stays here because reset timing, regions, influence, and garrisons are game behavior; Unityforge owns the frame callback.
pub fn tick() {
    if !crate::skills::SETTLED.load(std::sync::atomic::Ordering::Relaxed) {
        FORCES.lock().clear();
        REGIONS.lock().clear();
        INFLUENCE_HANDLE.store(0, Ordering::Relaxed);
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
/// Stays here because Schedule 1 defines factions, influence loss, takeover, and reward rolls; Modforge owns only shared primitives.
pub fn on_mob_down(npc_ptr: i64) -> Option<(f32, f32, String)> {
    let mut forces = FORCES.lock();
    let i = forces.iter().position(|m| m.npc_ptr == npc_ptr)?;
    let m = forces.remove(i);
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
    vlog(&format!(
        "mob down: {} (ptr={npc_ptr}) in {region_name}, {live_here} still alive here",
        m.label
    ));
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
        let before = get_influence(m.region_idx).unwrap_or(-1.0);
        vlog(&format!(
            "influence change: {region_name} before={before:.4} delta={delta:.4}"
        ));
        match change_influence(m.region_idx, -delta) {
            Ok(now_at) => {
                vlog(&format!(
                    "influence result: {region_name} now={now_at:.4} (was {before:.4})"
                ));
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
/// Stays here because the tally reports this mod's regional war state; Modforge owns no Schedule 1 region identity.
#[allow(dead_code)]
pub fn kills_by_region() -> Vec<(String, u32)> {
    KILLS_BY_REGION.lock().clone()
}

/// Rolls Schedule 1's delay before a defeated regional garrison can reinforce.
/// Stays here because the range and random pacing are game tuning, not framework behavior.
fn roll_reinforce() -> Duration {
    Duration::from_secs(fastrand::u64(REINFORCE_SECS.0..=REINFORCE_SECS.1))
}

// ---- game plumbing ---------------------------------------------------

/// Cached handle for the one live CartelInfluence instance.
/// Populated on first use, never released (the singleton lives
/// the whole session). Cleared to 0 when REGIONS reset (game
/// not settled / scene change).
static INFLUENCE_HANDLE: AtomicI32 = AtomicI32::new(0);

/// Finds and retains Schedule 1's live cartel-influence service for the current scene.
/// Stays here because the concrete class and scene lifetime are game facts; Unityforge owns type walking and handles.
fn cartel_influence_handle() -> Result<i32, String> {
    let h = INFLUENCE_HANDLE.load(Ordering::Relaxed);
    if h != 0 {
        return Ok(h);
    }
    let ty = MonoType::find("Il2CppScheduleOne.Cartel.CartelInfluence")
        .ok_or("CartelInfluence type not found")?;
    let walked = ty.walk(false)?;
    let ih = walked
        .as_array()
        .and_then(|a| a.first())
        .and_then(mono::json_handle)
        .ok_or("no live CartelInfluence")?;
    INFLUENCE_HANDLE.store(ih, Ordering::Relaxed);
    Ok(ih)
}

/// Reads the cartel's current influence value for one Schedule 1 region.
/// Stays here because the service method and region indexing belong to the game; Unityforge owns managed invocation.
fn get_influence(region_idx: usize) -> Result<f64, String> {
    let handle = cartel_influence_handle()?;
    Ok(mono::with_object(handle, |inst| {
        inst.invoke("GetInfluence", &json!([region_idx as i64]))
    })?
    .as_f64()
    .unwrap_or(0.0))
}

/// Move a region's influence through the game's own machinery
/// and report where it landed. Calls the RpcLogic directly
/// because the public ChangeInfluence is a FishNet ServerRpc
/// stub whose serialization round-trip silently drops our
/// invoke (the value never moves).
/// Stays here because the verified RpcLogic method is specific to Schedule 1; Unityforge owns generic method invocation.
fn change_influence(region_idx: usize, delta: f64) -> Result<f64, String> {
    let handle = cartel_influence_handle()?;
    mono::with_object(handle, |inst| {
        inst.invoke(
            "RpcLogic___ChangeInfluence_2792544924",
            &json!([region_idx as i64, delta]),
        )
    })?;
    get_influence(region_idx)
}

/// Discovers Schedule 1's regions, ranks, delivery anchors, owners, and initial garrison posts.
/// Stays here because every field and ownership threshold belongs to the game's war design; Unityforge owns collection access.
fn load_regions() -> Result<usize, String> {
    let map_ty = MonoType::find("Il2CppScheduleOne.Map.Map").ok_or("Map type not found")?;
    let map = map_ty.singleton_instance().ok_or("no Map singleton")?;
    let regions_v = map.read_field("Regions")?;
    let rh = mono::json_handle(&regions_v).ok_or("Regions carried no handle")?;
    let regions_arr = mono::owned_object(rh);
    let n = regions_arr
        .invoke("get_Length", &json!([]))?
        .as_i64()
        .ok_or("Regions length unreadable")?;
    let mut out = Vec::new();
    for i in 0..n {
        let item = regions_arr.invoke("get_Item", &json!([i]))?;
        let Some(reg_h) = mono::json_handle(&item) else {
            continue;
        };
        let reg = mono::owned_object(reg_h);
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
            if let Some(dlh) = mono::json_handle(&dl) {
                let list = mono::owned_object(dlh);
                let count = list
                    .invoke("get_Count", &json!([]))
                    .ok()
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                for j in 0..count {
                    let Ok(e) = list.invoke("get_Item", &json!([j])) else {
                        continue;
                    };
                    let Some(eh) = mono::json_handle(&e) else {
                        continue;
                    };
                    let loc = mono::owned_object(eh);
                    if let Ok(t) = loc.read_field("transform") {
                        if let Some(th) = mono::json_handle(&t) {
                            let tr = mono::owned_object(th);
                            if let Some((x, y, z)) = tr
                                .invoke("get_position", &json!([]))
                                .ok()
                                .and_then(|p| parse_vec3(&p))
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
/// Stays here because the influence and rank formula is Schedule 1 balance tuning, not a reusable framework rule.
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
/// Stays here because it composes Schedule 1 regions, cartel ownership, player position, and spawn policy; frameworks supply access.
fn war_pass() -> Result<(), String> {
    // Player position (for aggro checks only; garrisons do not
    // care where the player is).
    let player_ty =
        MonoType::find("Il2CppScheduleOne.PlayerScripts.Player").ok_or("Player type not found")?;
    let players = player_ty.walk(false)?;
    let player_h = players
        .as_array()
        .and_then(|a| a.first())
        .and_then(mono::json_handle)
        .ok_or("no live Player")?;
    let player = mono::owned_object(player_h);
    let ppos = {
        let transform = player.read_field("transform")?;
        let th = mono::json_handle(&transform).ok_or("no player transform")?;
        let t = mono::owned_object(th);
        parse_vec3(&t.invoke("get_position", &json!([]))?).ok_or("bad player position")?
    };

    // Supply is unlimited now (minted NPCs via the shim's
    // S1API-backed NpcFactory); TOTAL_LIVE_CAP is the guard.
    let now = Instant::now();
    let mut spawned = 0usize;
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
        if spawned >= SPAWNS_PER_PASS || FORCES.lock().len() >= TOTAL_LIVE_CAP {
            break;
        }
        match spawn_mob(idx, &name, post) {
            Ok(reinforcement) => {
                spawned += 1;
                if reinforcement {
                    mono::log(
                        LogLevel::Info,
                        &format!("schedule1-mod: the cartel reinforces {name}"),
                    );
                }
            }
            Err(e) => {
                mono::log(
                    LogLevel::Warn,
                    &format!("schedule1-mod: spawn in {name} failed: {e}"),
                );
                break;
            }
        }
    }

    hold_posts(ppos.0, ppos.2);
    Ok(())
}

/// Aggro pass over the standing forces: a mob whose POST is
/// within its rolled radius of the player gets the attack order
/// through the factory (minted NPCs idle at their posts;
/// BaseEmployee stock does not exit-walk like cartel goons, so
/// no hold orders needed).
/// Stays here because affix timing, attack range, and factory orders are Schedule 1 NPC behavior; Unityforge owns invocation.
fn hold_posts(px: f64, pz: f64) {
    let now = Instant::now();
    let mut orders: Vec<(usize, i64, String)> = Vec::new();
    let mut affix_work: Vec<(usize, i64, Option<f32>, Option<&'static str>)> = Vec::new();
    {
        let mut forces = FORCES.lock();
        for (i, g) in forces.iter_mut().enumerate() {
            // Affixes land once the S1API spawn pipeline settled.
            if !g.affixes_applied && now.duration_since(g.spawned_at) >= AFFIX_DELAY {
                g.affixes_applied = true;
                if g.pending_toughness.is_some() || g.pending_weapon.is_some() {
                    affix_work.push((i, g.minted_index, g.pending_toughness, g.pending_weapon));
                }
            }
            if g.aggroed || now.duration_since(g.last_order) < HOLD_ORDER_EVERY {
                continue;
            }
            g.last_order = now;
            let d_player = ((g.post.0 - px).powi(2) + (g.post.2 - pz).powi(2)).sqrt();
            if d_player <= g.aggro_radius {
                orders.push((i, g.minted_index, g.label.clone()));
            }
        }
    }
    for (_, minted, toughness, weapon) in affix_work {
        if let Some(t) = toughness {
            match factory_call("SetToughness", json!([minted, t])) {
                Ok(_) => vlog(&format!("affix: mint={minted} SetToughness({t:.0}) ok")),
                Err(e) => vlog(&format!("affix: mint={minted} SetToughness FAILED: {e}")),
            }
        }
        if let Some(w) = weapon {
            match factory_call("Arm", json!([minted, w])) {
                Ok(_) => vlog(&format!("affix: mint={minted} Arm({w}) ok")),
                Err(e) => vlog(&format!("affix: mint={minted} Arm FAILED: {e}")),
            }
        }
    }
    for (i, minted, label) in orders {
        if factory_call("AttackPlayer", json!([minted])).is_ok() {
            if let Some(g) = FORCES.lock().get_mut(i) {
                g.aggroed = true;
            }
            vlog(&format!("aggro: {label} (mint={minted}) ordered onto player"));
        }
    }
}

/// Spawn one cartel mob at a post via the minted-NPC factory.
/// Returns true when it was a reinforcement (the region already
/// took casualties).
/// Stays here because mob types, weapons, rewards, labels, and garrison membership are Schedule 1 content and balance.
fn spawn_mob(region_idx: usize, region_name: &str, post: (f64, f64, f64)) -> Result<bool, String> {
    let spawn = factory_call("SpawnGoon", json!([post.0, post.1, post.2]))?;
    let minted_index = spawn["index"].as_i64().ok_or("no mint index")?;
    let npc_ptr = spawn["ptr"].as_i64().filter(|p| *p != 0).ok_or("no npc ptr from factory")?;

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
    let mut pool_types = ["tough", "armed", "veteran"];
    fastrand::shuffle(&mut pool_types);

    let mut pending_toughness = None;
    let mut pending_weapon = None;
    for ty_name in pool_types.iter().take(affix_count) {
        match *ty_name {
            "tough" => {
                let mult = 2.0 + fastrand::f32() * 2.0;
                pending_toughness = Some(100.0 * mult);
                xp_mult += 1.0;
                loot_mult += mult * 0.5;
                names.push("Tough");
            }
            "armed" => {
                // The weapon roll: mostly melee, sometimes a gun.
                let w = fastrand::f32();
                let (weapon, bonus) = if w < 0.5 {
                    ("Avatar/Equippables/Baton", 0.5)
                } else if w < 0.85 {
                    ("Avatar/Equippables/Knife", 0.75)
                } else {
                    ("Avatar/Equippables/M1911", 2.0)
                };
                pending_weapon = Some(weapon);
                xp_mult += bonus;
                loot_mult += bonus;
                names.push("Armed");
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
    let aggro_radius = 14.0 + fastrand::f64() * 12.0;
    let reinforcement = REGIONS
        .lock()
        .get(region_idx)
        .map(|r| r.casualties > 0)
        .unwrap_or(false);
    FORCES.lock().push(Mob {
        region_idx,
        faction: Faction::Cartel,
        minted_index,
        npc_ptr,
        post,
        aggro_radius,
        aggroed: false,
        label: label.clone(),
        xp_mult,
        loot_mult,
        last_order: Instant::now(),
        spawned_at: Instant::now(),
        pending_toughness,
        pending_weapon,
        affixes_applied: false,
    });
    vlog(&format!(
        "spawned: {label} mint={minted_index} ptr={npc_ptr} in {region_name} at ({:.0},{:.0},{:.0}) aggro_r={aggro_radius:.0} xp={xp_mult:.2} loot={loot_mult:.2}",
        post.0, post.1, post.2
    ));
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
/// Stays here because the response presents Schedule 1's private war state; Modforge owns only operation registration and transport.
pub fn register_ops() {
    modforge::ops::OP_REGISTRY.register(modforge::ops::OpDef::new(
        "farm_state",
        "The war map: per region owner, influence, strength target, live force, casualties; live mob positions. No rolled specifics (spoiler firewall).",
        "{}",
        |_args| {
            // Posts (assigned stations), not live transforms:
            // minted NPCs idle at their posts.
            let mobs: Vec<serde_json::Value> = {
                let regions = REGIONS.lock();
                FORCES
                    .lock()
                    .iter()
                    .map(|g| {
                        let region = regions
                            .get(g.region_idx)
                            .map(|r| r.name.clone())
                            .unwrap_or_else(|| "?".into());
                        json!({
                            "region": region,
                            "post": {"x": g.post.0, "y": g.post.1, "z": g.post.2},
                            "aggroed": g.aggroed,
                        })
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
            let kills: Vec<serde_json::Value> = KILLS_BY_REGION
                .lock()
                .iter()
                .map(|(n, c)| json!({"region": n, "kills": c}))
                .collect();
            Ok(json!({"regions": per_region, "kills": kills, "mobs": mobs}))
        },
    ));
}
