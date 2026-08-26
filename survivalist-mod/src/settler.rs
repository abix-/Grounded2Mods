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

use std::sync::atomic::AtomicU32;

use parking_lot::Mutex;
use serde_json::json;

use modforge::mission::{self, OneStageStep};
use unityforge::mono::{self, LogLevel};

use crate::common::{base_centre, ctype, display_name, for_each_community, handle_of, own, with};

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
    if mission::should_tick(now, WATCH_TICK_SECS, &LAST_TICK_BITS) {
        mission::advance_one_stage_all(&MISSIONS, now, |mission, error| {
            mono::log(
                LogLevel::Warn,
                &format!(
                    "survivalist-mod: settler -- watch for {} failed: {error}",
                    mission.husk_name
                ),
            );
        });
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
    // Husks: dead camps with a real base rect, ripe to be claimed.
    let mut husks: Vec<Husk> = Vec::new();
    for_each_community(|com| {
        let t = ctype(&com);
        if t != "Normal" && t != "Looter" {
            return Ok(true);
        }
        if com
            .invoke("GetLivingNonZombieMemberCount", &json!([]))?
            .as_i64()
            .unwrap_or(0)
            > 0
        {
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
    if husks.is_empty() {
        return Ok(false);
    }

    // Spawn a REAL band at the edge from the undefined beyond (the
    // off-map generator + fresh-people/loot faucet).
    let salt = 40 + MISSIONS.lock().len() as u64;
    let Some((band_h, band_id, spawn_tile)) =
        crate::incursion::spawn_band_at_edge(now, salt, "RovingRefugee", 3, 6, false)?
    else {
        for h in &husks {
            drop(own(h.handle));
        }
        return Ok(false);
    };

    // Nearest husk to where they crossed.
    let mut best = 0usize;
    let mut best_d = i64::MAX;
    for (i, h) in husks.iter().enumerate() {
        let d = (h.centre.0 - spawn_tile.0).pow(2) + (h.centre.1 - spawn_tile.1).pow(2);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    let husk_h = husks[best].handle;
    let husk_id = husks[best].id;
    let husk_centre = husks[best].centre;
    let husk_name = husks[best].name.clone();

    // The game's own gate: dead, real base rect, not already claimed.
    let occupiable = with(husk_h, |h| {
        h.invoke("CanBeOccupiedBy", &json!([{ "handle": band_h }]))
    })? == json!(true);
    if !occupiable {
        drop(own(band_h));
        for h in &husks {
            drop(own(h.handle));
        }
        return Ok(false);
    }

    // Give the band an Occupy squad pointed at the husk; the game
    // walks them there and its own OccupyBase does the transfer.
    let goal = json!({"x": husk_centre.0, "y": husk_centre.1});
    let setup = with(band_h, |com| -> Result<(), String> {
        let squad_h = handle_of(&com.invoke("AddSquad", &json!(["Occupy", husk_id]))?)
            .ok_or("AddSquad gave no squad")?;
        if let Some(m_h) = handle_of(&com.read_field("Members")?) {
            let mlist = own(m_h);
            let n = mlist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
            for i in 0..n {
                if let Some(h) = handle_of(&mlist.invoke("get_Item", &json!([i]))?) {
                    let _ = com.invoke(
                        "AddToSquad",
                        &json!([{ "handle": h }, { "handle": squad_h }]),
                    );
                }
            }
        }
        let squad = own(squad_h);
        squad.write_field("Behaviour", &json!("Occupy"))?;
        squad.write_field("EnemyCommunityId", &json!(husk_id))?;
        squad.write_field("GoalTile", &goal)?;
        drop(squad);
        com.invoke(
            "SetSquadAction",
            &json!([{ "handle": squad_h }, "GoTo", 0, goal, null, false]),
        )?;
        drop(own(squad_h));
        Ok(())
    });
    if setup.is_err() {
        drop(own(band_h));
        for h in &husks {
            drop(own(h.handle));
        }
        return Ok(false);
    }

    MISSIONS.lock().push(Mission {
        group_h: band_h,
        group_id: band_id,
        husk_h,
        husk_name: husk_name.clone(),
        deadline: now + MISSION_TIMEOUT_SECS,
    });
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: settler -- a band crossed the edge to claim the dead base of {husk_name}"
        ),
    );

    // Release the husk handles we did not keep as the target.
    for (i, h) in husks.iter().enumerate() {
        if i != best {
            drop(own(h.handle));
        }
    }
    Ok(true)
}

impl mission::OneStageMission for Mission {
    fn advance(&mut self, now: f32) -> Result<OneStageStep, String> {
        let community_type = with(self.group_h, ctype);
        if community_type == "Normal" || community_type == "Looter" {
            let name = with(self.group_h, display_name);
            crate::chronicle::post(&format!(
                "a new banner flies over the dead walls of {}: outsiders settled there as {}",
                self.husk_name, name
            ));
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: settler -- SETTLED: {} claimed the dead base of {}",
                    name, self.husk_name
                ),
            );
            return Ok(OneStageStep::Complete);
        }
        let members = with(self.group_h, |group| {
            group
                .invoke("GetLivingNonZombieMemberCount", &json!([]))
                .ok()
                .and_then(|value| value.as_i64())
        })
        .unwrap_or(0);
        if members <= 0 {
            crate::chronicle::post("the settlers never made it; the road ate them");
            mono::log(
                LogLevel::Info,
                "survivalist-mod: settler -- the settling group died on the road",
            );
            return Ok(OneStageStep::Complete);
        }
        if now >= self.deadline {
            return Ok(OneStageStep::TimedOut);
        }
        Ok(OneStageStep::Continue)
    }

    fn on_timeout(&mut self, _now: f32) -> Result<(), String> {
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: settler -- stopped watching the walk to {} (the game still owns the squad)",
                self.husk_name
            ),
        );
        Ok(())
    }

    fn cleanup(self) {
        drop(own(self.group_h));
        drop(own(self.husk_h));
    }

    fn label(&self) -> String {
        format!("settlers bound for {}", self.husk_name)
    }
}
