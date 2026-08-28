//! Find and set the player's money/cash field.
//!
//! ```text
//! FISH_DEBUG_PORT=17174 cargo test -p fish-mod --test give_money -- --ignored --test-threads=1 --nocapture
//! ```

mod common;
use common::api_or_skip;
use serde_json::json;

#[test]
#[ignore]
fn find_money_field() {
    let Some(api) = api_or_skip() else { return };

    let candidates = [
        "PlayerKillScore",
        "PlayerMoney",
        "MoneyManager",
        "CurrencyManager",
        "Wallet",
        "Bank",
        "Economy",
        "ScoreManager",
        "PlayerScore",
        "GameScore",
        "SavedPlayer",
        "LocalUI",
        "PlayerUI",
    ];

    for sub in candidates {
        let r = api.op("walk_class", json!({"class": sub, "max": 1}));
        if !r.ok {
            continue;
        }
        let instances = r.result["instances"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if instances.is_empty() {
            continue;
        }
        println!("\n--- {sub}: {} instance(s) ---", instances.len());
        let handle = instances[0]["handle"].as_i64().unwrap();
        let inspect = api.op("inspect_object", json!({"handle": handle}));
        if inspect.ok {
            let props = inspect.result["properties"].as_array().unwrap();
            for p in props {
                let name = p["name"].as_str().unwrap_or("?");
                let lower = name.to_lowercase();
                let interesting = lower.contains("money")
                    || lower.contains("cash")
                    || lower.contains("gold")
                    || lower.contains("coin")
                    || lower.contains("score")
                    || lower.contains("kill")
                    || lower.contains("worth")
                    || lower.contains("balance")
                    || lower.contains("currency")
                    || lower.contains("save")
                    || lower.contains("slot");
                if interesting {
                    println!(
                        "  ** {name}: {} = {}",
                        p["type"].as_str().unwrap_or("?"),
                        p["value"]
                    );
                }
            }
        }
        api.op("release_handle", json!({"handle": handle}));
    }

    // get MoneyManager instance
    let r = api.op("walk_class", json!({"class": "MoneyManager", "max": 1}));
    assert!(r.ok, "walk_class MoneyManager failed");
    let mm_handle = r.result["instances"][0]["handle"].as_i64().unwrap();

    // read current money
    let r = api.op(
        "invoke_static",
        json!({"class": "MoneyManager", "method": "get_Money", "args": []}),
    );
    let before = r.result.as_i64().unwrap_or(0);
    println!("current money: {before}");

    // set to 300
    let target = 300_i64;
    let r = api.op(
        "invoke_static",
        json!({"class": "MoneyManager", "method": "set_Money", "args": [target]}),
    );
    assert!(r.ok, "set_Money failed: {:?}", r.error);

    // fire the SyncVar change callback so the UI updates
    let r = api.op(
        "invoke_method",
        json!({"handle": mm_handle, "method": "OnChangeMoney", "args": [before, target, false]}),
    );
    println!("OnChangeMoney: ok={} error={:?}", r.ok, r.error);

    // also try asServer=true in case that's needed for the host
    let r = api.op(
        "invoke_method",
        json!({"handle": mm_handle, "method": "OnChangeMoney", "args": [before, target, true]}),
    );
    println!(
        "OnChangeMoney(asServer=true): ok={} error={:?}",
        r.ok, r.error
    );

    // also poke MoneyUI directly
    let r = api.op("walk_class", json!({"class": "MoneyUI", "max": 1}));
    if r.ok {
        let ui_instances = r.result["instances"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !ui_instances.is_empty() {
            let ui_handle = ui_instances[0]["handle"].as_i64().unwrap();
            println!("\nMoneyUI found, inspecting...");
            let inspect = api.op("inspect_object", json!({"handle": ui_handle}));
            if inspect.ok {
                let props = inspect.result["properties"].as_array().unwrap();
                for p in props {
                    let name = p["name"].as_str().unwrap_or("?");
                    let lower = name.to_lowercase();
                    if lower.contains("money")
                        || lower.contains("text")
                        || lower.contains("update")
                        || lower.contains("display")
                        || lower.contains("label")
                        || lower.contains("ui")
                    {
                        println!(
                            "  {name}: {} = {}",
                            p["type"].as_str().unwrap_or("?"),
                            p["value"]
                        );
                    }
                }
            }
            println!("\nMoneyUI methods:");
            modforge::client::print_declared_methods(&api, "MoneyUI");
            api.op("release_handle", json!({"handle": ui_handle}));
        }
    }

    // verify
    let r = api.op(
        "invoke_static",
        json!({"class": "MoneyManager", "method": "get_Money", "args": []}),
    );
    println!("\nfinal money: {}", r.result);
    api.op("release_handle", json!({"handle": mm_handle}));
}
