//! A settling faction: the off-map incursion that founds a new camp
//! (docs/status.md, the storyteller backlog), so the map is never
//! fully known and a husk is a real power vacuum.
//!
//! Nothing is conjured. The group is the same real roving-refugee
//! inflow the strangers claim; the base is a real husk (a dead
//! settlement with a real base rect, left behind when a faction was
//! destroyed); and the claim is the game's own reclamation path.
//! The mod points the group's traveling squad at the husk with the
//! vanilla occupy shape (Behaviour=Occupy, EnemyCommunityId,
//! GoalTile; the same fields GoToNextTradeDestination sets), the
//! game walks them there on its own AI, and on arrival its own
//! OccupyBase transfers the base rect, buildings, and crops, renames
//! the group, and flips it into a real Normal or Looter settlement.
//! A hot reload loses only the watch: the squad is fully
//! vanilla-owned, so the settling still completes on its own.

use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::json;

use unityforge::mono::{self, LogLevel};

use crate::common::{
    base_centre, ctype, display_name, for_each_community, handle_of, own, parse_xy, with,
};

/// Seconds between watch passes.
const WATCH_TICK_SECS: f32 = 10.0;

/// Stop watching after this long; the game's own AI still owns the
/// squad, so a slow walk can still settle unannounced.
const MISSION_TIMEOUT_SECS: f32 = 3600.0;

/// One settling faction in flight at a time.
const MAX_SETTLERS: usize = 1;

struct Mission {
    group_h: i32,
    group_id: i64,
    husk_h: i32,
    husk_name: String,
    deadline: f32,
}

static MISSIONS: Mutex<Vec<Mission>> = Mutex::new(Vec::new());
static LAST_TICK_BITS: AtomicU32 = AtomicU32::new(0);

/// Groups this system owns; growth.rs and the strangers skip them
/// so nothing recruits or re-rolls a band mid-walk.
pub fn is_claimed(id: i64) -> bool {
    MISSIONS.lock().iter().any(|m| m.group_id == id)
}

pub fn active_count() -> usize {
    MISSIONS.lock().len()
}

pub fn tick(now: f32) {
    let last = f32::from_bits(LAST_TICK_BITS.load(Ordering::Relaxed));
    if now - last >= WATCH_TICK_SECS {
        LAST_TICK_BITS.store(now.to_bits(), Ordering::Relaxed);
        advance(now);
    }
}

/// Force-launch a settling faction now: the closest unclaimed
/// arriving group is pointed at a claimable husk. Returns whether a
/// group and a husk matched. The incursion loop drives this, so
/// every settling arrives foreshadowed by off-map dread.
pub fn launch_now(now: f32) -> bool {
    match launch(now) {
        Ok(launched) => launched,
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: settler -- launch failed: {e}"),
            );
            false
        }
    }
}

struct Group {
    handle: i32,
    id: i64,
    pos: (f32, f32),
}

struct Husk {
    handle: i32,
    id: i64,
    name: String,
    centre: (i64, i64),
}

fn launch(now: f32) -> Result<bool, String> {
    if MISSIONS.lock().len() >= MAX_SETTLERS {
        return Ok(false);
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut husks: Vec<Husk> = Vec::new();
    for_each_community(|com| {
        let t = ctype(&com);
        if t == "RovingRefugee" {
            let members = com
                .invoke("GetLivingNonZombieMemberCount", &json!([]))?
                .as_i64()
                .unwrap_or(0);
            let id = com.read_field("Id")?.as_i64().unwrap_or(-1);
            if members > 0 && !is_claimed(id) && !crate::stranger::is_claimed(id) {
                if let Some(lead_h) = handle_of(&com.read_field("Leader")?) {
                    // Tile coords, to match the husks' base centres.
                    let pos = own(lead_h).read_field("Tile").ok().and_then(|v| parse_xy(&v));
                    if let Some(pos) = pos {
                        groups.push(Group {
                            handle: com.handle().0,
                            id,
                            pos,
                        });
                        std::mem::forget(com);
                        return Ok(true);
                    }
                }
            }
            return Ok(true);
        }
        if t != "Normal" && t != "Looter" {
            return Ok(true);
        }
        let members = com
            .invoke("GetLivingNonZombieMemberCount", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        if members > 0 {
            return Ok(true);
        }
        let Some(centre) = base_centre(&com) else {
            return Ok(true);
        };
        let id = com.read_field("Id")?.as_i64().unwrap_or(-1);
        husks.push(Husk {
            handle: com.handle().0,
            id,
            name: display_name(&com),
            centre,
        });
        std::mem::forget(com);
        Ok(true)
    })?;

    let result = try_launch(&groups, &husks, now);

    let kept: Vec<i32> = {
        let ms = MISSIONS.lock();
        ms.iter().flat_map(|m| [m.group_h, m.husk_h]).collect()
    };
    for g in &groups {
        if !kept.contains(&g.handle) {
            drop(own(g.handle));
        }
    }
    for h in &husks {
        if !kept.contains(&h.handle) {
            drop(own(h.handle));
        }
    }
    result
}

fn try_launch(groups: &[Group], husks: &[Husk], now: f32) -> Result<bool, String> {
    if groups.is_empty() || husks.is_empty() {
        return Ok(false);
    }
    let mut best: Option<(&Group, &Husk, f32)> = None;
    for g in groups {
        for h in husks {
            let (dx, dy) = (g.pos.0 - h.centre.0 as f32, g.pos.1 - h.centre.1 as f32);
            let d2 = dx * dx + dy * dy;
            if best.map(|(_, _, bd)| d2 < bd).unwrap_or(true) {
                best = Some((g, h, d2));
            }
        }
    }
    let Some((group, husk, _)) = best else {
        return Ok(false);
    };
    // The game's own gate: dead, real base rect, nobody else already
    // claiming it, the Nemesis rule.
    let occupiable = with(husk.handle, |h| {
        h.invoke("CanBeOccupiedBy", &json!([{ "handle": group.handle }]))
    })? == json!(true);
    if !occupiable {
        return Ok(false);
    }
    // The group's traveling squad, re-pointed with the vanilla
    // occupy shape; the game walks them there and OccupyBase does
    // the transfer on arrival.
    let squad_h = with(group.handle, |g| -> Result<Option<i32>, String> {
        let Some(s_h) = handle_of(&g.read_field("Squads")?) else {
            return Ok(None);
        };
        let slist = own(s_h);
        if slist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0) == 0 {
            return Ok(None);
        }
        Ok(handle_of(&slist.invoke("get_Item", &json!([0]))?))
    })?;
    let Some(squad_h) = squad_h else {
        return Ok(false);
    };
    let goal = json!({"x": husk.centre.0, "y": husk.centre.1});
    let squad = own(squad_h);
    squad.write_field("Behaviour", &json!("Occupy"))?;
    squad.write_field("EnemyCommunityId", &json!(husk.id))?;
    squad.write_field("GoalTile", &goal)?;
    drop(squad);
    with(group.handle, |g| {
        g.invoke(
            "SetSquadAction",
            &json!([{ "handle": squad_h }, "GoTo", 0, goal, null, false]),
        )
    })?;
    drop(own(squad_h));
    MISSIONS.lock().push(Mission {
        group_h: group.handle,
        group_id: group.id,
        husk_h: husk.handle,
        husk_name: husk.name.clone(),
        deadline: now + MISSION_TIMEOUT_SECS,
    });
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: settler -- a group is walking to claim the dead base of {}",
            husk.name
        ),
    );
    Ok(true)
}

fn advance(now: f32) {
    let mut missions = MISSIONS.lock();
    let mut i = 0;
    while i < missions.len() {
        if resolve(&missions[i], now) {
            let m = missions.remove(i);
            drop(own(m.group_h));
            drop(own(m.husk_h));
        } else {
            i += 1;
        }
    }
}

fn resolve(m: &Mission, now: f32) -> bool {
    // OccupyBase flips the group's type when the claim lands.
    let t = with(m.group_h, ctype);
    if t == "Normal" || t == "Looter" {
        let name = with(m.group_h, display_name);
        crate::chronicle::post(&format!(
            "a new banner flies over the dead walls of {}: outsiders settled there as {}",
            m.husk_name, name
        ));
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: settler -- SETTLED: {} claimed the dead base of {}",
                name, m.husk_name
            ),
        );
        return true;
    }
    let members = with(m.group_h, |g| {
        g.invoke("GetLivingNonZombieMemberCount", &json!([]))
            .ok()
            .and_then(|v| v.as_i64())
    })
    .unwrap_or(0);
    if members <= 0 {
        crate::chronicle::post("the settlers never made it; the road ate them");
        mono::log(
            LogLevel::Info,
            "survivalist-mod: settler -- the settling group died on the road",
        );
        return true;
    }
    if now >= m.deadline {
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: settler -- stopped watching the walk to {} (the game still owns the squad)",
                m.husk_name
            ),
        );
        return true;
    }
    false
}
