//! The work pillar's payment courier (docs/status.md "More to do
//! (ecosystem-generated work)"): when a debt to the player comes
//! due, the hirer loads real non-food stacks from its stores onto
//! its first free member, who walks them to the player's gate as
//! a real 1-member Trade squad, hands them into storage, and
//! walks home. Shared by every work kind (the bounty, clearing a
//! threat); each kind keeps its own state machine and embeds a
//! Courier while paying.

use serde_json::{Value as Json, json};

pub use modforge::mission::Stage;
use modforge::mission::{self, Step};
use unityforge::mono::{self, LogLevel, MonoObject};

use crate::common::{
    GoodsFilter, base_centre, carry_off_stored_goods, ctype, for_each_community, handle_of, own,
    with,
};

/// Non-food stacks a payment carries; also the cap the offers
/// advertise as pays.
pub const PAY_STACKS: i64 = 3;

/// A courier that has not resolved by then is recalled.
const COURIER_TIMEOUT_SECS: f32 = 1800.0;

/// Within this squared tile distance of a building the courier
/// has arrived; same bar trade uses.
const ARRIVE_DIST_SQ: f64 = 25.0;

/// A payment on the road. Owns hirer_h, courier_h, and player_h;
/// step() releases all three when the run ends.
pub struct Courier {
    pub hirer_h: i32,
    pub hirer_name: String,
    pub courier_h: i32,
    pub courier_name: String,
    pub player_h: i32,
    pub squad_id: i64,
    pub home: (i64, i64),
    pub stage: Stage,
    pub loaded: i64,
    pub deadline: f32,
}

/// Outcome of a launch attempt. On Launched the Courier takes
/// ownership of hirer_h; on Waiting and Void the CALLER keeps it
/// (and on Void closes out the debt).
pub enum Launch {
    Launched(Courier),
    /// No free member today; keep the debt and retry next pass.
    Waiting,
    /// The debt is void: hirer dead, no player camp, bare stores,
    /// or the squad would not form.
    Void,
}

/// Load the payment and put the courier on the road.
/// `debt_name` names what is being paid for in log lines ("the
/// bounty on X", "the raiders at X's door").
/// Stays here because it applies Survivalist's payment couriers rules through the game's classes, fields, content, and actions.
pub fn launch(hirer_h: i32, hirer_name: &str, debt_name: &str, now: f32) -> Launch {
    // The player's gate.
    let mut player: Option<(i32, (i64, i64))> = None;
    let _ = for_each_community(|com| {
        if ctype(&com) == "Player" {
            if let Some(c) = base_centre(&com) {
                player = Some((com.handle().0, c));
                std::mem::forget(com);
            }
            return Ok(false);
        }
        Ok(true)
    });
    let Some((player_h, dest)) = player else {
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: courier: no player camp to pay; {hirer_name}'s debt is void"
            ),
        );
        return Launch::Void;
    };
    // A hirer that died owing pays nothing.
    let standing = with(hirer_h, |c| {
        c.invoke("HasAnyLivingNonZombieMembers", &json!([]))
            .map(|v| v == json!(true))
            .unwrap_or(false)
    });
    if !standing {
        mono::log(
            LogLevel::Info,
            &format!("survivalist-mod: courier: {hirer_name} died owing; the debt dies with them"),
        );
        drop(own(player_h));
        return Launch::Void;
    }
    // The courier: the first free member.
    let courier = match with(hirer_h, |com| pick_courier(com)) {
        Ok(c) => c,
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: courier: pick failed: {e}"),
            );
            None
        }
    };
    let Some((courier_h, courier_name)) = courier else {
        drop(own(player_h));
        return Launch::Waiting;
    };
    // Load the payment from real stores.
    let loaded = with(hirer_h, |com| {
        carry_off_stored_goods(com, &[courier_h], PAY_STACKS, GoodsFilter::NonFood, false)
    })
    .unwrap_or(0);
    if loaded == 0 {
        crate::chronicle::post(&format!("{hirer_name} cannot pay what they owe"));
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: courier: {hirer_name}'s stores are bare; the debt for {debt_name} goes unpaid"
            ),
        );
        drop(own(courier_h));
        drop(own(player_h));
        return Launch::Void;
    }
    // On the road as a real 1-member Trade squad.
    let home = with(hirer_h, |com| base_centre(com)).unwrap_or(dest);
    let dest_j = json!({"x": dest.0, "y": dest.1});
    let squad_id = match with(hirer_h, |com| -> Result<i64, String> {
        let squad_h = handle_of(&com.invoke("AddSquad", &json!(["Trade", 0]))?)
            .ok_or("AddSquad gave no squad")?;
        let squad = own(squad_h);
        com.invoke(
            "AddToSquad",
            &json!([{ "handle": courier_h }, { "handle": squad_h }]),
        )?;
        squad.write_field("GoalTile", &dest_j)?;
        com.invoke(
            "SetSquadAction",
            &json!([{ "handle": squad_h }, "GoTo", 0, dest_j.clone(), null, false]),
        )?;
        squad.read_field("Id").map(|v| v.as_i64().unwrap_or(-1))
    }) {
        Ok(id) => id,
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("survivalist-mod: courier: launch failed: {e}"),
            );
            drop(own(courier_h));
            drop(own(player_h));
            return Launch::Void;
        }
    };
    crate::chronicle::post(&format!("{hirer_name} sends payment for {debt_name}"));
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: courier: {hirer_name} sends {courier_name} with {loaded} stack(s) of payment to the player's gate"
        ),
    );
    Launch::Launched(Courier {
        hirer_h,
        hirer_name: hirer_name.to_string(),
        courier_h,
        courier_name,
        player_h,
        squad_id,
        home,
        stage: Stage::Going,
        loaded,
        deadline: now + COURIER_TIMEOUT_SECS,
    })
}

/// One courier step. None = the run ended (paid, lost, or
/// recalled) and every handle is released.
/// Stays here because it applies Survivalist's payment couriers rules through the game's classes, fields, content, and actions.
pub fn step(c: Courier, now: f32) -> Option<Courier> {
    mission::advance_owned(c, now, |_courier, error| {
        mono::log(
            LogLevel::Warn,
            &format!("survivalist-mod: courier: tile read failed: {error}"),
        );
    })
}

impl mission::Mission for Courier {
    modforge::mission_accessors!();

    /// Check whether the mission agent can continue.
    /// Stays here because it applies Survivalist's payment couriers rules through the game's classes, fields, content, and actions.
    fn is_agent_alive(&self) -> Result<bool, String> {
        let alive = with(self.courier_h, |ch| {
            ch.invoke("get_AliveAndNotZombie", &json!([]))
        })
        .map(|value| value == json!(true))
        .unwrap_or(false);
        if !alive {
            crate::chronicle::post(&format!(
                "the payment from {} never arrived",
                self.hirer_name
            ));
            mono::log(
                LogLevel::Info,
                &format!(
                    "survivalist-mod: courier: {} died on the road; the payment is lost",
                    self.courier_name
                ),
            );
        }
        Ok(alive)
    }

    /// Resolve what happens when the mission reaches its destination.
    /// Stays here because it applies Survivalist's payment couriers rules through the game's classes, fields, content, and actions.
    fn on_going(&mut self, _now: f32) -> Result<Step, String> {
        let tile = with(self.courier_h, |ch| ch.invoke("get_Tile", &json!([])))?;
        let distance = with(self.player_h, |player| {
            player.invoke("GetDistSqToNearestBuilding", &json!([tile]))
        })
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(f64::MAX);
        if distance > ARRIVE_DIST_SQ {
            return Ok(Step::Continue);
        }
        let delivered =
            deliver_carried_payment(self.courier_h, self.player_h, self.loaded).unwrap_or(0);
        if delivered > 0 {
            crate::chronicle::post(&format!(
                "a courier from {} brings your payment",
                self.hirer_name
            ));
        }
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: courier: {} delivers {delivered} stack(s) into the player's store",
                self.courier_name
            ),
        );
        let home = json!({"x": self.home.0, "y": self.home.1});
        let _ = with(self.hirer_h, |community| -> Result<(), String> {
            if let Ok(squad) = community.invoke("GetSquad", &json!([self.squad_id])) {
                if let Some(squad_h) = handle_of(&squad) {
                    let squad = own(squad_h);
                    squad.write_field("GoalTile", &home)?;
                    community.invoke(
                        "SetSquadAction",
                        &json!([{ "handle": squad_h }, "GoTo", 0, home.clone(), null, false]),
                    )?;
                }
            }
            Ok(())
        });
        self.loaded = delivered;
        Ok(Step::Transition)
    }

    /// Resolve what happens when the mission agent returns home.
    /// Stays here because it applies Survivalist's payment couriers rules through the game's classes, fields, content, and actions.
    fn on_returning(&mut self, _now: f32) -> Result<Step, String> {
        let tile = with(self.courier_h, |ch| ch.invoke("get_Tile", &json!([])))?;
        let distance = with(self.hirer_h, |community| {
            community.invoke("GetDistSqToNearestBuilding", &json!([tile]))
        })
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(f64::MAX);
        if distance > ARRIVE_DIST_SQ {
            return Ok(Step::Continue);
        }
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: courier: {} home; the debt is paid and closed",
                self.courier_name
            ),
        );
        Ok(Step::Complete)
    }

    /// Resolve a mission that ran out of time.
    /// Stays here because it applies Survivalist's payment couriers rules through the game's classes, fields, content, and actions.
    fn on_timeout(&mut self, _now: f32) -> Result<(), String> {
        mono::log(
            LogLevel::Info,
            &format!(
                "survivalist-mod: courier: {} recalled (timeout); the payment never arrived",
                self.courier_name
            ),
        );
        Ok(())
    }

    /// Release the mission squad and managed handles when the mission ends.
    /// Stays here because it applies Survivalist's payment couriers rules through the game's classes, fields, content, and actions.
    fn cleanup(self) {
        close(&self);
    }

    /// Describe the active work for status output.
    /// Stays here because it applies Survivalist's payment couriers rules through the game's classes, fields, content, and actions.
    fn label(&self) -> String {
        format!("payment from {}", self.hirer_name)
    }
}

/// Disband the courier squad and release every held handle.
/// Stays here because it applies Survivalist's payment couriers rules through the game's classes, fields, content, and actions.
fn close(c: &Courier) {
    with(c.hirer_h, |com| {
        if let Ok(sq) = com.invoke("GetSquad", &json!([c.squad_id])) {
            if let Some(sq_h) = handle_of(&sq) {
                let _ = com.invoke("RemoveSquad", &json!([{ "handle": sq_h }]));
            }
        }
    });
    drop(own(c.hirer_h));
    drop(own(c.courier_h));
    drop(own(c.player_h));
}

/// The first free member: alive, human, conscious, unsquadded,
/// not the leader (murder.rs's eligibility, no genome ranking).
/// Stays here because it applies Survivalist's payment couriers rules through the game's classes, fields, content, and actions.
fn pick_courier(com: &MonoObject) -> Result<Option<(i32, String)>, String> {
    let leader_id = handle_of(&com.read_field("Leader")?).map(|h| {
        own(h)
            .read_field("Id")
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(-1)
    });
    let Some(m_h) = handle_of(&com.read_field("Members")?) else {
        return Ok(None);
    };
    let mlist = own(m_h);
    let count = mlist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
    for i in 0..count {
        let Some(h) = handle_of(&mlist.invoke("get_Item", &json!([i]))?) else {
            continue;
        };
        let member = own(h);
        let alive = member
            .invoke("get_AliveAndNotZombie", &json!([]))
            .map(|v| v == json!(true))
            .unwrap_or(false);
        let human = member
            .invoke("GetBaseObjectType", &json!([]))
            .map(|v| v == json!("Human"))
            .unwrap_or(false);
        let conscious = member
            .invoke("get_IsConscious", &json!([]))
            .map(|v| v == json!(true))
            .unwrap_or(false);
        let squadded =
            handle_of(&member.invoke("GetSquad", &json!([])).unwrap_or(Json::Null)).is_some();
        let id = member
            .read_field("Id")
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(-1);
        if !alive || !human || !conscious || squadded || Some(id) == leader_id {
            continue;
        }
        let name = member
            .invoke("GetDisplayNameString", &json!([]))
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "<unnamed>".into());
        std::mem::forget(member);
        return Ok(Some((h, name)));
    }
    Ok(None)
}

/// Move up to `max` non-food stacks from the courier's carried
/// inventory into the player's first storage building: the
/// payout, on the same Take/Add calls as everything else
/// (trade.rs's delivery, filter inverted).
/// Stays here because it applies Survivalist's payment couriers rules through the game's classes, fields, content, and actions.
fn deliver_carried_payment(courier_h: i32, player_h: i32, max: i64) -> Result<i64, String> {
    // The receiving shelf: the player's first building with an
    // inventory container.
    let store: Option<(i32, i32)> = with(player_h, |host| {
        let b_h = handle_of(&host.read_field("Buildings").ok()?)?;
        let blist = own(b_h);
        let nb = blist.invoke("get_Count", &json!([])).ok()?.as_i64()?;
        for bi in 0..nb {
            let Some(bh) = handle_of(&blist.invoke("get_Item", &json!([bi])).ok()?) else {
                continue;
            };
            let building = own(bh);
            if let Some(inv_h) = handle_of(&building.read_field("Inventory").ok()?) {
                std::mem::forget(building);
                return Some((bh, inv_h));
            }
        }
        None
    });
    let Some((store_bh, store_inv_h)) = store else {
        return Ok(0);
    };
    let courier_inv_h = with(courier_h, |c| {
        handle_of(&c.read_field("Inventory")?).ok_or("courier has no inventory".to_string())
    })?;
    let courier_inv = own(courier_inv_h);
    let store_inv = own(store_inv_h);
    let mut delivered = 0i64;
    while delivered < max {
        let count = courier_inv
            .invoke("get_Count", &json!([]))
            .ok()
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let mut pick: Option<(i32, i64)> = None;
        for i in 0..count {
            let Some(item_h) = handle_of(&courier_inv.invoke("GetItem", &json!([i]))?) else {
                continue;
            };
            let item = own(item_h);
            let amount = item
                .invoke("GetAmount", &json!([]))
                .ok()
                .and_then(|v| v.as_i64())
                .unwrap_or(1);
            if GoodsFilter::NonFood.matches(&item) {
                std::mem::forget(item);
                pick = Some((item_h, amount));
                break;
            }
        }
        let Some((item_h, amount)) = pick else { break };
        let taken = courier_inv.invoke(
            "Take",
            &json!([{ "handle": courier_h }, { "handle": item_h }, amount]),
        )?;
        let Some(taken_h) = handle_of(&taken) else {
            break;
        };
        store_inv.invoke(
            "Add",
            &json!([{ "handle": store_bh }, { "handle": taken_h }]),
        )?;
        delivered += 1;
    }
    drop(courier_inv);
    drop(store_inv);
    drop(own(store_bh));
    Ok(delivered)
}
