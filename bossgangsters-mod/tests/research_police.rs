//! What are the LIVE police tuning values?
//!
//! The decompile gives the code defaults; the prefab can differ.
//! Reads the police relationship, the per-crime score table, and
//! the PoliceCrimeSettings off the running game.
//!
//! ```text
//! cargo test -p bossgangsters-mod --test research_police -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, ping_or_skip};
use serde_json::json;

#[test]
fn police_live_tuning() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }
    let handle_in = |res: &serde_json::Value| res.get("handle").and_then(|h| h.as_i64());

    let Some(police) = unityforge::client::first_handle_inactive(&api, "PoliceManager") else {
        println!("PoliceManager: no live instance");
        return;
    };

    let rel = api.op("read_field", json!({"handle": police, "field": "relationship"}));
    if rel.ok {
        if let Some(h) = handle_in(&rel.result) {
            let v = api.op("read_field", json!({"handle": h, "field": "Value"}));
            println!("police relationship = {}", v.result);
        } else {
            println!("police relationship = {}", rel.result);
        }
    }

    let table = api.op("read_field", json!({"handle": police, "field": "crimeTable"}));
    if table.ok {
        if let Some(h) = handle_in(&table.result) {
            let dump = api.op("inspect_object", json!({"handle": h}));
            println!("crimeTable: {}", dump.result);
        } else {
            println!("crimeTable = {}", table.result);
        }
    } else {
        println!("crimeTable: read failed ({:?})", table.error);
    }

    // Per-crime scores: crimeTable[CrimeType] -> CrimeData
    // {score, cooldown}. CrimeType 0..12 (13 entries counted).
    if let Some(h) = table.ok.then(|| handle_in(&table.result)).flatten() {
        let names = [
            "PedestrianKill", "DrugDealer", "DrugDealAttempt", "HumanTrafficker", "CarSteal",
            "ClubRaid", "TributeCapture", "TaxiScam", "Pickpocket", "PoliceShoot", "PoliceKill",
            "VehicleExplosion", "IllegalDrinkFatality",
        ];
        for (i, name) in names.iter().enumerate() {
            let item = api.op("invoke_method", json!({"handle": h, "method": "get_Item", "args": [i]}));
            let Some(data) = item.ok.then(|| handle_in(&item.result)).flatten() else {
                println!("crime {name}: not in table");
                continue;
            };
            let score = api.op("read_field", json!({"handle": data, "field": "score"}));
            let cooldown = api.op("read_field", json!({"handle": data, "field": "cooldown"}));
            println!("crime {name}: score {} cooldown {}", score.result, cooldown.result);
        }
    }

    let Some(coord) = unityforge::client::first_handle_inactive(&api, "PoliceCrimeCoordinator")
    else {
        println!("PoliceCrimeCoordinator: no live instance");
        return;
    };
    let settings = api.op("read_field", json!({"handle": coord, "field": "settings"}));
    if let Some(h) = settings.ok.then(|| handle_in(&settings.result)).flatten() {
        for field in [
            "detectionRadius",
            "arrestDistance",
            "chaseCooldownDuration",
            "wantedDuration",
            "shootingStartDelay",
            "shootingInterval",
            "shotDamage",
            "shotMissChance",
            "enableChaseShooting",
        ] {
            let r = api.op("read_field", json!({"handle": h, "field": field}));
            println!("settings.{field} = {}", if r.ok { r.result } else { json!(r.error) });
        }
    } else {
        println!("settings: read failed ({:?})", settings.error);
    }
}
