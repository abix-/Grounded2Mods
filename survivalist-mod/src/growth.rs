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

use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::bridge::MonoHandle;
use unityforge::hook::{self, HOOK_REGISTRY};
use unityforge::mono::{self, LogLevel, MonoObject, MonoType};

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
}

pub fn register_ops() {
    OP_REGISTRY.register_many([OpDef::new(
        "growth_status",
        "Per-settlement population, beds, nutrition, rebuild/repair queues, plus refugee groups in transit. The growth observability surface.",
        "{}",
        growth_status,
    )]);
}

extern "C" fn suppress_repopulation(_ctx: *const c_void) -> i32 {
    1 // skip the original: the conjurer never runs
}

// ---- shared helpers (same conventions as war.rs) ---------------------------

/// SAFETY: caller asserts the handle came fresh out of a bridge
/// response and is not wrapped anywhere else; Drop releases it.
fn own(h: i32) -> MonoObject {
    unsafe { MonoObject::from_handle(MonoHandle(h)) }
}

fn handle_of(v: &Json) -> Option<i32> {
    v.get("handle").and_then(Json::as_i64).map(|h| h as i32)
}

fn community_manager() -> Result<MonoObject, String> {
    let session = MonoType::find("Session")
        .and_then(|t| t.singleton_instance())
        .ok_or("Session.Instance not found (no game loaded?)")?;
    let cm_h = handle_of(&session.read_field("CommunityManager")?)
        .ok_or("Session.CommunityManager is null")?;
    Ok(own(cm_h))
}

fn for_each_community(mut f: impl FnMut(MonoObject) -> Result<bool, String>) -> Result<(), String> {
    let cm = community_manager()?;
    let list_h = handle_of(&cm.read_field("Communities")?).ok_or("Communities list is null")?;
    let list = own(list_h);
    let count = list
        .invoke("get_Count", &json!([]))?
        .as_i64()
        .ok_or("get_Count did not return a number")?;
    for i in 0..count {
        let Some(item_h) = handle_of(&list.invoke("get_Item", &json!([i]))?) else {
            continue;
        };
        if !f(own(item_h))? {
            break;
        }
    }
    Ok(())
}

fn display_name(com: &MonoObject) -> String {
    com.invoke("GetDisplayNameString", &json!([]))
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "<unnamed>".to_string())
}

fn ctype(com: &MonoObject) -> String {
    com.read_field("CommunityType")
        .map(|v| v.as_str().unwrap_or("?").to_string())
        .unwrap_or_else(|_| "?".to_string())
}

fn list_len(owner: &MonoObject, field: &str) -> i64 {
    match owner.read_field(field).ok().as_ref().and_then(handle_of) {
        Some(h) => own(h)
            .invoke("get_Count", &json!([]))
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        None => 0,
    }
}

/// Parse a struct's ToString of the shape "(x, y)" (Vector2 /
/// TerrainCoord); the bridge serializes value types that way.
fn parse_xy(v: &Json) -> Option<(f32, f32)> {
    let s = v.as_str()?;
    let s = s.trim().trim_start_matches('(').trim_end_matches(')');
    let mut it = s.split(',');
    let x = it.next()?.trim().parse::<f32>().ok()?;
    let y = it.next()?.trim().parse::<f32>().ok()?;
    Some((x, y))
}

fn pos_of(obj: &MonoObject) -> Option<(f32, f32)> {
    if let Ok(v) = obj.read_field("PosXZ") {
        if let Some(p) = parse_xy(&v) {
            return Some(p);
        }
    }
    obj.read_field("Tile").ok().and_then(|v| parse_xy(&v))
}

// ---- recruitment tick -------------------------------------------------------

/// f32 bits of the last scan's `now`; on_tick is main-thread
/// only, the atomic is just for the static.
static LAST_SCAN_BITS: AtomicU32 = AtomicU32::new(0);

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
    pos: (f32, f32),
    headroom: i64,
}

fn recruit_scan() -> Result<(), String> {
    // Pass 1: settlements with an open door (doctrine v1: Normal
    // only, bed headroom, fed) and refugee groups in transit.
    let mut doors: Vec<OpenDoor> = Vec::new();
    let mut refugees: Vec<MonoObject> = Vec::new();
    for_each_community(|com| {
        let t = ctype(&com);
        if t == "RovingRefugee" {
            if com
                .invoke("GetLivingNonZombieMemberCount", &json!([]))?
                .as_i64()
                .unwrap_or(0)
                > 0
            {
                refugees.push(com);
            }
            return Ok(true);
        }
        if t != "Normal" {
            return Ok(true); // Looter doctrine: nobody welcome (v1)
        }
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
        let beds = com.invoke("GetAccommodation", &json!([]))?.as_i64().unwrap_or(0);
        let headroom = beds - members;
        if headroom <= 0 {
            return Ok(true);
        }
        let nutrition = com
            .invoke("CalcCommunityNutritionLevel", &json!([0.0]))?
            .as_f64()
            .unwrap_or(0.0);
        if nutrition < RECRUIT_MIN_NUTRITION {
            return Ok(true);
        }
        // Anchor position: the first building.
        let Some(b_h) = com
            .read_field("Buildings")
            .ok()
            .as_ref()
            .and_then(handle_of)
        else {
            return Ok(true);
        };
        let blist = own(b_h);
        if blist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0) == 0 {
            return Ok(true);
        }
        let Some(anchor_h) = handle_of(&blist.invoke("get_Item", &json!([0]))?) else {
            return Ok(true);
        };
        let Some(pos) = pos_of(&own(anchor_h)) else {
            return Ok(true);
        };
        let name = display_name(&com);
        doors.push(OpenDoor {
            com,
            name,
            pos,
            headroom,
        });
        Ok(true)
    })?;
    if doors.is_empty() || refugees.is_empty() {
        return Ok(());
    }

    // Pass 2: any refugee group standing near an open door walks
    // in (as many as there are beds).
    for group in refugees {
        let lead_h = match handle_of(&group.read_field("Leader")?) {
            Some(h) => h,
            None => continue,
        };
        let Some(gpos) = pos_of(&own(lead_h)) else {
            continue;
        };
        for door in doors.iter_mut() {
            if door.headroom <= 0 {
                continue;
            }
            let (dx, dy) = (gpos.0 - door.pos.0, gpos.1 - door.pos.1);
            if dx * dx + dy * dy > RECRUIT_RANGE * RECRUIT_RANGE {
                continue;
            }
            let moved = absorb_group(&group, door)?;
            if moved > 0 {
                mono::log(
                    LogLevel::Info,
                    &format!(
                        "survivalist-mod: growth -- {} takes in {} refugee(s) who arrived at their gate ({} bed(s) left)",
                        door.name,
                        moved,
                        door.headroom
                    ),
                );
            }
            break;
        }
    }
    Ok(())
}

/// Move up to `door.headroom` living members of `group` into the
/// settlement via the game's own join path. Returns how many
/// moved.
fn absorb_group(group: &MonoObject, door: &mut OpenDoor) -> Result<i64, String> {
    let Some(m_h) = handle_of(&group.read_field("Members")?) else {
        return Ok(0);
    };
    let mlist = own(m_h);
    let count = mlist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
    // Collect handles first: SetCommunity mutates the source list.
    let mut joiners: Vec<i32> = Vec::new();
    for i in 0..count {
        if (joiners.len() as i64) >= door.headroom {
            break;
        }
        if let Some(h) = handle_of(&mlist.invoke("get_Item", &json!([i]))?) {
            let member = own(h);
            let alive = member.invoke("get_AliveAndNotZombie", &json!([]));
            let alive = match alive {
                Ok(v) => v == json!(true),
                Err(_) => member.read_field("Alive").map(|v| v == json!(true)).unwrap_or(false),
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
        moved += 1;
        door.headroom -= 1;
    }
    Ok(moved)
}

// ---- observability ----------------------------------------------------------

fn growth_status(_args: &Json) -> Result<Json, String> {
    use std::sync::Arc;

    use parking_lot::Mutex;
    let result: Arc<Mutex<Option<Result<Json, String>>>> = Arc::new(Mutex::new(None));
    let r2 = result.clone();
    unityforge::main_thread_queue::MAIN_QUEUE.push(move || {
        *r2.lock() = Some(collect_growth_status());
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(r) = result.lock().take() {
            return r;
        }
        if std::time::Instant::now() >= deadline {
            return Err("growth_status: main-thread queue timed out".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

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
        let beds = com.invoke("GetAccommodation", &json!([]))?.as_i64().unwrap_or(0);
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
            "rebuild_queue": list_len(&com, "ConstructionRecords"),
            "repair_queue": list_len(&com, "NeedsRepair"),
        }));
        Ok(true)
    })?;
    Ok(json!({"settlements": settlements, "refugee_groups_in_transit": arrivals}))
}
