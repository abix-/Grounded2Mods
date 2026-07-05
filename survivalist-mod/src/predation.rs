//! Predation: the Darwinian selection event (docs/faction-war.md
//! "THE VISION: a Darwinian world").
//!
//! NOT territorial takeover (operator-locked 2026-07-05): a winner
//! does NOT move into the loser's base. It STRIPS the loser of
//! everything portable, people and stored goods, brings it home
//! to its OWN base, and leaves an empty husk that dies. Full
//! stakes, life or death, no real estate.
//!
//! - CONSUME THE PEOPLE: the beaten camp's survivors are absorbed
//!   into the winner via the game's own `SetCommunity` (the
//!   press-gang move). They walk to the winner's base carrying
//!   their inventories, so portable wealth comes home for free.
//! - STRIP THE GOODS: the loser's stored world items are
//!   transferred to the winner (ownership via SetCommunity). No
//!   teleport, no territory: buildings/crops/land stay with the
//!   dead husk (they cannot be carried).
//! - EXTINCTION: emptied of people, the loser hits zero members
//!   and the game's own death fires. With the conjurer dead
//!   (growth.rs), it stays gone.
//! - SELECTION + HEREDITY: the winner's genome lives and spreads
//!   (more bodies carry it); the loser's genome is removed from
//!   the pool (`genome::remove`). Unfit trait sets die out; fit
//!   ones propagate. Evolution closes the loop here.
//!
//! Trigger: a faction whose invasion target has been beaten down
//! to a last handful (<= 2 living, or nobody conscious) consumes
//! it. One conquest per scan keeps the map's consolidation
//! dramatic and legible.

use serde_json::json;

use unityforge::mono::{self, LogLevel, MonoObject};

use crate::common::{ctype, display_name, for_each_community, handle_of, own};
use crate::genome;

/// A beaten camp with this many or fewer living members is ripe
/// to be consumed.
const CONSUME_AT_OR_BELOW: i64 = 2;

/// Called from the survival tick (main thread already). Finds one
/// conquest-ready war and resolves it by predation.
pub fn check_conquests() -> Result<(), String> {
    // Find the first winner whose invasion target is beaten.
    let mut winner_h: Option<i32> = None;
    let mut loser_h: Option<i32> = None;
    for_each_community(|com| {
        let t = ctype(&com);
        if t != "Normal" && t != "Looter" {
            return Ok(true);
        }
        if com.invoke("IsAISettlement", &json!([]))? != json!(true) {
            return Ok(true);
        }
        let winner_members = com
            .invoke("GetLivingNonZombieMemberCount", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        if winner_members == 0 {
            return Ok(true);
        }
        let Some(target_h) = handle_of(&com.read_field("InvasionTarget")?) else {
            return Ok(true);
        };
        let target = own(target_h);
        let loser_members = target
            .invoke("GetLivingNonZombieMemberCount", &json!([]))?
            .as_i64()
            .unwrap_or(0);
        // 1-2 survivors clinging on = ripe. Zero = already dead,
        // nothing to consume. Also require somebody unconscious/few
        // so we don't consume a healthy camp mid-war.
        let beaten = loser_members >= 1
            && (loser_members <= CONSUME_AT_OR_BELOW
                || target.invoke("IsAnyoneConscious", &json!([false]))? == json!(false));
        if beaten && loser_members >= 1 {
            winner_h = Some(com.handle().0);
            loser_h = Some(target_h);
            std::mem::forget(com);
            std::mem::forget(target);
            return Ok(false);
        }
        Ok(true)
    })?;

    let (Some(wh), Some(lh)) = (winner_h, loser_h) else {
        return Ok(());
    };
    consume(own(wh), own(lh))
}

fn consume(winner: MonoObject, loser: MonoObject) -> Result<(), String> {
    let winner_name = display_name(&winner);
    let loser_name = display_name(&loser);
    let loser_id = loser.read_field("Id")?.as_i64().unwrap_or(-1);

    // 1. Consume the people: absorb every living survivor. Keep
    // their handles: they are the carriers who haul the loot home.
    let mut carriers: Vec<i32> = Vec::new();
    if let Some(m_h) = handle_of(&loser.read_field("Members")?) {
        let mlist = own(m_h);
        let count = mlist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
        // Collect first: SetCommunity mutates the source list.
        let mut joiners: Vec<i32> = Vec::new();
        for i in 0..count {
            if let Some(h) = handle_of(&mlist.invoke("get_Item", &json!([i]))?) {
                let member = own(h);
                let alive = member
                    .invoke("get_AliveAndNotZombie", &json!([]))
                    .map(|v| v == json!(true))
                    .unwrap_or(false);
                let human = member
                    .invoke("GetBaseObjectType", &json!([]))
                    .map(|v| v == json!("Human"))
                    .unwrap_or(false);
                if alive && human {
                    joiners.push(h);
                    std::mem::forget(member);
                } // non-human (animals) left with the husk
            }
        }
        for h in joiners {
            let member = own(h);
            if member
                .invoke("SetCommunity", &json!([{ "handle": winner.handle().0 }]))
                .is_ok()
            {
                // Absorbed by force = conscript. In a Looter victor
                // they will never vote (its predatory will stays
                // the conquerors'); a Normal victor lets everyone
                // vote, so the flag only silences them under Looter
                // rule.
                if let Some(id) = member.read_field("Id").ok().and_then(|v| v.as_i64()) {
                    genome::mark_conscript(id);
                }
                carriers.push(h);
            }
            std::mem::forget(member); // reused as a carrier below
        }
    }
    let absorbed = carriers.len() as i64;

    // 2. LOOT THE STOCKPILE, carried not cheated: move each item
    // stored in the loser's buildings into an absorbed survivor's
    // OWN inventory (the game's honest Take/Add transfer, subject
    // to real carry capacity, verified APIs). The survivors then
    // physically walk that loot to the winner's base. Nothing is
    // duplicated or teleported; wealth conserved, hands do the
    // carrying. If nobody survived to carry, the stockpile is
    // left in the husk (no cheat-grab).
    let goods = if carriers.is_empty() {
        0
    } else {
        loot_buildings(&loser, &carriers)?
    };

    // 3. Selection: the loser's genome dies with it. (The people it
    // lost now carry the winner's genome by belonging to the
    // winner; the winner's trait set propagates.)
    genome::remove(loser_id);

    let wg = genome::get_or_seed(
        winner.read_field("Id")?.as_i64().unwrap_or(-1),
        &ctype(&winner),
    );
    mono::log(
        LogLevel::Info,
        &format!(
            "survivalist-mod: PREDATION -- {winner_name} (aggression {:.2}) consumed {loser_name}: absorbed {absorbed} survivor(s), who carried off {goods} looted good(s). {loser_name} is EXTINCT.",
            wg.get(genome::Trait::Aggression)
        ),
    );
    Ok(())
}

/// Move the loser's building-stored items into the absorbed
/// survivors' inventories, round-robin, via the game's own
/// `EquipmentContainer.Take` (removes from source) + `Add` (gives
/// to carrier, honoring real carry capacity). Returns how many
/// items were carried off. Buildings/crops stay with the husk;
/// only portable stored goods move, and only as far as living
/// hands can carry them.
fn loot_buildings(loser: &MonoObject, carriers: &[i32]) -> Result<i64, String> {
    let Some(b_h) = handle_of(&loser.read_field("Buildings")?) else {
        return Ok(0);
    };
    let blist = own(b_h);
    let nb = blist.invoke("get_Count", &json!([]))?.as_i64().unwrap_or(0);
    let mut carried = 0i64;
    let mut carrier_ix = 0usize;
    for bi in 0..nb {
        let Some(bh) = handle_of(&blist.invoke("get_Item", &json!([bi]))?) else {
            continue;
        };
        let building = own(bh);
        let Some(inv_h) = handle_of(&building.read_field("Inventory")?) else {
            continue;
        };
        let inv = own(inv_h);
        // Drain the container from the top; Take() shrinks it, so
        // re-read Count each pass and always take index 0.
        loop {
            let count = inv.invoke("Count", &json!([])).ok();
            // Count is a property (get_Count) on EquipmentContainer.
            let count = match count {
                Some(v) if v.is_i64() => v.as_i64().unwrap(),
                _ => inv
                    .invoke("get_Count", &json!([]))
                    .ok()
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
            };
            if count <= 0 {
                break;
            }
            let Some(item_h) = handle_of(&inv.invoke("GetItem", &json!([0]))?) else {
                break;
            };
            let item = own(item_h);
            let amount = item.invoke("GetAmount", &json!([])).ok().and_then(|v| v.as_i64()).unwrap_or(1);
            // Take the whole stack from the building.
            let taken = inv.invoke(
                "Take",
                &json!([{ "handle": bh }, { "handle": item_h }, amount]),
            )?;
            let Some(taken_h) = handle_of(&taken) else {
                break; // Take failed; stop draining this building
            };
            // Hand it to a carrier (round-robin). Add honors carry
            // capacity; anything that doesn't fit is dropped at the
            // site by the game, which is realistic (they took what
            // they could carry).
            let carrier = own(carriers[carrier_ix % carriers.len()]);
            let _ = carrier.invoke(
                "Add",
                &json!([{ "handle": carrier.handle().0 }, { "handle": taken_h }]),
            );
            std::mem::forget(carrier);
            carrier_ix += 1;
            carried += 1;
            if carried >= 500 {
                return Ok(carried); // safety cap; a camp never holds this much
            }
        }
    }
    Ok(carried)
}
