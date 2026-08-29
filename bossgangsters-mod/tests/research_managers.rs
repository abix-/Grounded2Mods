//! Do the player, money, and game-manager classes named by the
//! decompile exist in the live game?
//!
//! From the ilspycmd decompile of Assembly-CSharp (all in the
//! Tycoon namespace, all MonoSingleton<T>):
//! - ClubPlayer: owns the player (`playerBot`, referenced 135
//!   times across the game's own code)
//! - MoneyManager: owns the money (`money` int field, AddMoney /
//!   SpendMoney / AddMoneyForEditor methods)
//! - GameManager: owns game state (CurrentGameState,
//!   gameStateChanged)
//!
//! ```text
//! cargo test -p bossgangsters-mod --test research_managers -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, ping_or_skip};
use serde_json::json;
use unityforge::client::first_handle_inactive as first_handle;

#[test]
fn player_money_and_game_manager_exist() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    for class in ["ClubPlayer", "MoneyManager", "GameManager"] {
        match first_handle(&api, class) {
            Some(handle) => println!("{class}: live instance, handle {handle}"),
            None => println!("{class}: NO LIVE INSTANCE"),
        }
    }

    if let Some(money_mgr) = first_handle(&api, "MoneyManager") {
        let read = api.op("read_field", json!({"handle": money_mgr, "field": "money"}));
        if read.ok {
            println!("MoneyManager.money = {}", read.result);
        } else {
            println!("MoneyManager.money: read failed ({:?})", read.error);
        }
    }

    if let Some(player) = first_handle(&api, "ClubPlayer") {
        let read = api.op("read_field", json!({"handle": player, "field": "playerBot"}));
        if read.ok {
            println!("ClubPlayer.playerBot = {}", read.result);
        } else {
            println!("ClubPlayer.playerBot: read failed ({:?})", read.error);
        }
    }
}
