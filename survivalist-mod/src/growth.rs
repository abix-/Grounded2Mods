//! REAL growth, phase: people (docs/faction-war.md scorecard
//! pillars "Town growth" + "No cheating").
//!
//! Two halves:
//!
//! 1. SUPPRESS THE CONJURER (operator-locked): a prefix skips
//!    `CommunityManager.UpdateRepopulation` entirely, so the
//!    game never again materializes a person out of thin air
//!    inside a settlement. KNOWN SIDE EFFECT, documented in
//!    faction-war.md: the same method also refills roving trader
//!    parties and chickens; both are suppressed too pending the
//!    operator's two open boundary calls.
//!
//! 2. RECRUITMENT of real arrivals: roving refugee groups (the
//!    template-spawned inflow that walks onto the map) get
//!    absorbed by a settlement when they are physically NEAR its
//!    base and it can take them. Per-type doctrine v1:
//!    - Normal settlements welcome refugees when they have bed
//!      space (accommodation headroom) and food (nutrition >=
//!      0.5, the game's own hunger-pressure line).
//!    - Looter settlements take nobody in v1 (their recruitment
//!      personality is an open doctrine question with the
//!      operator).
//!    Population is now bounded by REAL beds, not by the worldgen
//!    headcount: this is the first mechanism that can grow a camp
//!    past its starting size, and it only moves people who
//!    actually walked here.
//!
//! The join uses the game's own machinery: `Character.
//! SetCommunity(settlement)` (routes through `Community.
//! AddMember`) + `Community.UpdateRoles(newcomer)`, the exact
//! post-join wiring the vanilla repopulator used.
//!
//! Op `growth_status`: per-settlement population, beds,
//! nutrition, rebuild/repair queues, plus refugee groups in
//! transit. The observability surface for this pillar.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::{Value as Json, json};

use modforge::faction::first_reachable_destination;
use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::hook::{self, HOOK_REGISTRY, HookCtx};
use unityforge::mono::{self, LogLevel, MonoObject};

use crate::common::{ctype, display_name, for_each_community, handle_of, own, pos_of};

/// Seconds between recruitment scans (real time; the scan is a
/// few dozen reflection reads on the main thread).
const RECRUIT_SCAN_PERIOD_SECS: f32 = 15.0;

/// A refugee group joins a settlement when its leader stands
/// within this many world units of one of the settlement's
/// buildings (the game's own "close to base" conversations use
/// 32; a bit more lets groups camped just outside count).
const RECRUIT_RANGE: f32 = 48.0;

/// Normal settlements only take people in when they can feed
/// them: nutrition level at or above the game's own
/// hunger-pressure line (below 0.5 vanilla settlements start
/// begging).
const RECRUIT_MIN_NUTRITION: f64 = 0.5;

/// Install the game hooks that activate this system.
/// Stays here because it applies Survivalist's faction growth rules through the game's classes, fields, content, and actions.
pub fn install() {
    match hook::patch_prefix(
        "CommunityManager",
        "UpdateRepopulation",
        suppress_repopulation,
    ) {
        Ok(h) => {
            HOOK_REGISTRY.register(h);
            mono::log(
                LogLevel::Info,
                "survivalist-mod: growth -- repopulator DISABLED (UpdateRepopulation prefix skip); people now only arrive on foot",
            );
        }
        Err(e) => {
            mono::log(
                LogLevel::Error,
                &format!("survivalist-mod: growth repopulator patch FAILED: {e}"),
            );
        }
    }

    // Cheat 2 of 3: ambient enemy spawn points REFILL raider camps
    // after their people die. The gate method itself separates the
    // first spawn (worldgen population, kept) from the respawn
    // (LastDiedTime set, the conjure): suppress only the respawn.
    // A skipped bool method returns false = cannot spawn.
    match hook::patch_prefix_ctx(
        "AmbientEnemySpawnPoint",
        "CanSpawn",
        HookCtx::Instance,
        suppress_enemy_respawn,
    ) {
        Ok(h) => {
            HOOK_REGISTRY.register(h);
            mono::log(
                LogLevel::Info,
                "survivalist-mod: growth -- raider respawns DISABLED (AmbientEnemySpawnPoint.CanSpawn prefix; first spawns kept, refills suppressed)",
            );
        }
        Err(e) => {
            mono::log(
                LogLevel::Error,
                &format!("survivalist-mod: growth raider-respawn patch FAILED: {e}"),
            );
        }
    }
}

/// How many raider-camp refills have been suppressed this
/// generation (shown by growth_status; a log per attempt would
/// spam since the game polls the gate).
static RESPAWNS_SUPPRESSED: AtomicU32 = AtomicU32::new(0);

/// Stop a destroyed hostile camp from receiving free replacement survivors.
/// Stays here because it applies Survivalist's faction growth rules through the game's classes, fields, content, and actions.
extern "C" fn suppress_enemy_respawn(ctx: *const c_void) -> i32 {
    let h = ctx as isize as i32;
    if h == 0 {
        return 0; // no instance; run the original
    }
    let point = own(h);
    let died = point
        .read_field("LastDiedTime")
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    if died > 0.0 {
        RESPAWNS_SUPPRESSED.fetch_add(1, Ordering::Relaxed);
        return 1; // skip: a dead raider camp stays dead
    }
    0 // first spawn: the world may feed the map
}

/// Expose this system status and controls through the mod control endpoint.
/// Stays here because it applies Survivalist's faction growth rules through the game's classes, fields, content, and actions.
pub fn register_ops() {
    OP_REGISTRY.register_many([OpDef::new(
        "growth_status",
        "Per-settlement population, beds, nutrition, rebuild/repair queues, plus refugee groups in transit. The growth observability surface.",
        "{}",
        growth_status,
    )]);
}

/// Stop the game from silently refilling empty settlements.
/// Stays here because it applies Survivalist's faction growth rules through the game's classes, fields, content, and actions.
extern "C" fn suppress_repopulation(_ctx: *const c_void) -> i32 {
    1 // skip the original: the conjurer never runs
}

// ---- recruitment tick -------------------------------------------------------

/// f32 bits of the last scan's `now`; on_tick is main-thread
/// only, the atomic is just for the static.
static LAST_SCAN_BITS: AtomicU32 = AtomicU32::new(0);

/// Advance this system when its scheduled game update is due.
/// Stays here because it applies Survivalist's faction growth rules through the game's classes, fields, content, and actions.
pub fn tick(now: f32) {
    let last = f32::from_bits(LAST_SCAN_BITS.load(Ordering::Relaxed));
    if now - last < RECRUIT_SCAN_PERIOD_SECS {
        return;
    }
    LAST_SCAN_BITS.store(now.to_bits(), Ordering::Relaxed);
    if let Err(e) = recruit_scan() {
        // "no game loaded" between menus is normal; stay quiet
        // unless a real failure shape shows up in the log.
        if !e.contains("not found") {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: growth recruit scan failed: {e}"),
            );
        }
    }
}

struct OpenDoor {
    com: MonoObject,
    name: String,
    /// The settlement's base anchor plus, for looters, every
    /// roaming squad leader's position (press-gang reach).
    anchors: Vec<(f32, f32)>,
    headroom: i64,
    /// Looter press-gang vs Normal welcome (log wording +
    /// doctrine gates differ).
    press_gang: bool,
}

/// Find a camp entrance where willing recruits can join.
/// Stays here because it applies Survivalist's faction growth rules through the game's classes, fields, content, and actions.
fn base_anchor(com: &MonoObject) -> Option<(f32, f32)> {
    let b_h = com
        .read_field("Buildings")
        .ok()
        .as_ref()
        .and_then(handle_of)?;
    let blist = own(b_h);
    if blist.list_len_or_zero().ok()? == 0 {
        return None;
    }
    let anchor_h = blist.list_handle(0).ok().flatten()?;
    pos_of(&own(anchor_h))
}

/// Positions of every squad leader the community has in the
/// field. This is the looter press-gang reach: they grow through
/// activity, not hospitality.
/// Stays here because it applies Survivalist's faction growth rules through the game's classes, fields, content, and actions.
fn squad_anchors(com: &MonoObject) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let Some(sq_h) = com.read_field("Squads").ok().as_ref().and_then(handle_of) else {
        return out;
    };
    let sq_list = own(sq_h);
    let n = sq_list.list_len().unwrap_or(0);
    for i in 0..n {
        let Some(s_h) = sq_list.list_handle(i).ok().flatten() else {
            continue;
        };
        let squad = own(s_h);
        if let Ok(leader_j) = squad.invoke("GetLeader", &json!([])) {
            if let Some(l_h) = handle_of(&leader_j) {
                if let Some(p) = pos_of(&own(l_h)) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// Find nearby survivors that a camp can honestly recruit.
/// Stays here because Survivalist decides eligibility, observes the
/// game, and transfers members; Modforge matches arrivals to capacity.
fn recruit_scan() -> Result<(), String> {
    // Pass 1: settlements that can take people in, and refugee
    // groups in transit. Doctrine (operator-locked): Normal camps
    // WELCOME at the gate (beds + fed); Looter camps PRESS-GANG
    // near their base or any roaming squad (beds required, food
    // not checked: they take people hungry and raid for the
    // rest).
    let mut doors: Vec<OpenDoor> = Vec::new();
    let mut refugees: Vec<MonoObject> = Vec::new();
    for_each_community(|com| {
        let t = ctype(&com);
        if t == "RovingRefugee" {
            let members = com
                .invoke("GetLivingNonZombieMemberCount", &json!([]))?
                .as_i64()
                .unwrap_or(0);
            let id = com.read_field("Id")?.as_i64().unwrap_or(-1);
            // A band the stranger or settler system has claimed is
            // its to resolve; do not recruit it out from under the
            // roll or the walk to a claimable base.
            if members > 0 && !crate::stranger::is_claimed(id) && !crate::settler::is_claimed(id) {
                refugees.push(com);
            }
            return Ok(true);
        }
        let press_gang = match t.as_str() {
            "Normal" => false,
            "Looter" => true,
            _ => return Ok(true),
        };
        if com.invoke("IsAISettlement", &json!([]))? != json!(true) {
            return Ok(true);
        }
        let members = com
            .invoke("GetLivingNonZombieMemberCount", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        if members == 0 {
            return Ok(true); // dead camps do not recruit
        }
        let beds = com
            .invoke("GetAccommodation", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        let headroom = beds - members;
        if headroom <= 0 {
            return Ok(true);
        }
        if !press_gang {
            let nutrition = com
                .invoke("CalcCommunityNutritionLevel", &json!([0.0]))?
                .as_f64()
                .unwrap_or(0.0);
            if nutrition < RECRUIT_MIN_NUTRITION {
                return Ok(true);
            }
        }
        let mut anchors = Vec::new();
        if let Some(p) = base_anchor(&com) {
            anchors.push(p);
        }
        if press_gang {
            anchors.extend(squad_anchors(&com));
        }
        if anchors.is_empty() {
            return Ok(true);
        }
        let name = display_name(&com);
        doors.push(OpenDoor {
            com,
            name,
            anchors,
            headroom,
            press_gang,
        });
        Ok(true)
    })?;
    if doors.is_empty() || refugees.is_empty() {
        return Ok(());
    }

    // Pass 2: any refugee group within reach of a door is taken
    // in (as many as there are beds).
    for group in refugees {
        let lead_h = match handle_of(&group.read_field("Leader")?) {
            Some(h) => h,
            None => continue,
        };
        let Some(gpos) = pos_of(&own(lead_h)) else {
            continue;
        };
        let Some(destination) = first_reachable_destination(
            gpos,
            RECRUIT_RANGE,
            doors.iter().enumerate().map(|(index, door)| {
                (index, door.headroom.max(0) as usize, door.anchors.as_slice())
            }),
        ) else {
            continue;
        };
        let door = &mut doors[destination];
        let moved = absorb_group(&group, door)?;
        if moved > 0 {
            let verb = if door.press_gang {
                "press-gangs"
            } else {
                "takes in"
            };
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: growth -- {} {} {} refugee(s) ({} bed(s) left)",
                    door.name, verb, moved, door.headroom
                ),
            );
        }
    }
    Ok(())
}

/// Move up to `door.headroom` living members of `group` into the
/// settlement via the game's own join path. Returns how many
/// moved.
/// Stays here because it invokes Survivalist's member and role APIs
/// and applies the mod's conscript rule.
fn absorb_group(group: &MonoObject, door: &mut OpenDoor) -> Result<i64, String> {
    let Some(m_h) = handle_of(&group.read_field("Members")?) else {
        return Ok(0);
    };
    let mlist = own(m_h);
    let count = mlist.list_len_or_zero()?;
    // Collect handles first: SetCommunity mutates the source list.
    let mut joiners: Vec<i32> = Vec::new();
    for i in 0..count {
        if (joiners.len() as i64) >= door.headroom {
            break;
        }
        if let Some(h) = mlist.list_handle(i)? {
            let member = own(h);
            let alive = member.invoke("get_AliveAndNotZombie", &json!([]));
            let alive = match alive {
                Ok(v) => v == json!(true),
                Err(_) => member
                    .read_field("Alive")
                    .map(|v| v == json!(true))
                    .unwrap_or(false),
            };
            if alive {
                joiners.push(member.handle().0);
                std::mem::forget(member); // keep for the join pass
            }
        }
    }
    let mut moved = 0i64;
    for h in joiners {
        let member = own(h);
        member.invoke("SetCommunity", &json!([{ "handle": door.com.handle().0 }]))?;
        let _ = door
            .com
            .invoke("UpdateRoles", &json!([{ "handle": member.handle().0 }]));
        // Press-ganged = taken by force = conscript (voiceless in a
        // Looter faction). A welcomed refugee at a Normal camp is
        // NOT marked: they earned a voice. Only the seized are
        // silenced.
        if door.press_gang {
            if let Some(id) = member.read_field("Id").ok().and_then(|v| v.as_i64()) {
                crate::genome::mark_conscript(id);
            }
        }
        moved += 1;
        door.headroom -= 1;
    }
    Ok(moved)
}

// ---- observability ----------------------------------------------------------

/// Return the current faction-growth report.
/// Stays here because it applies Survivalist's faction growth rules through the game's classes, fields, content, and actions.
fn growth_status(_args: &Json) -> Result<Json, String> {
    unityforge::main_thread_queue::MAIN_QUEUE.run_result(
        "growth_status",
        std::time::Duration::from_secs(5),
        collect_growth_status,
    )
}

/// Count surviving camps, population, beds, and blocked free respawns.
/// Stays here because it applies Survivalist's faction growth rules through the game's classes, fields, content, and actions.
fn collect_growth_status() -> Result<Json, String> {
    let mut settlements = Vec::new();
    let mut arrivals = Vec::new();
    for_each_community(|com| {
        let t = ctype(&com);
        if t == "RovingRefugee" {
            let members = com
                .invoke("GetLivingNonZombieMemberCount", &json!([]))?
                .as_i64()
                .unwrap_or(0);
            if members > 0 {
                arrivals.push(json!({"members": members}));
            }
            return Ok(true);
        }
        if t != "Normal" && t != "Looter" && t != "Player" {
            return Ok(true);
        }
        let members = com
            .invoke("GetLivingNonZombieMemberCount", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        if members == 0 && t != "Player" {
            return Ok(true);
        }
        let beds = com
            .invoke("GetAccommodation", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        let nutrition = com
            .invoke("CalcCommunityNutritionLevel", &json!([0.0]))?
            .as_f64()
            .unwrap_or(0.0);
        settlements.push(json!({
            "name": display_name(&com),
            "type": t,
            "members": members,
            "initial_members": com.read_field("InitialMemberCount").unwrap_or(Json::Null),
            "beds": beds,
            "nutrition": (nutrition * 100.0).round() / 100.0,
            "rebuild_queue": com.field_list_len("ConstructionRecords"),
            "repair_queue": com.field_list_len("NeedsRepair"),
        }));
        Ok(true)
    })?;
    Ok(json!({
        "settlements": settlements,
        "refugee_groups_in_transit": arrivals,
        "raider_respawns_suppressed": RESPAWNS_SUPPRESSED.load(Ordering::Relaxed),
    }))
}
