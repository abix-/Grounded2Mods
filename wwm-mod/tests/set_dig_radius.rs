//! Set the equipped shovel's dig radius, the stat the game shows
//! as "dig radius" (`ui_dig_radius`) in the shovel upgrade popup.
//!
//! This writes `ToolDataSO.radius` on the currently equipped
//! tool. That is a ScriptableObject, not save data: the write
//! hits the in-memory asset shared by every use of that tool, so
//! it holds across scene and save loads for as long as the game
//! process lives. It is not expected to survive a restart, since
//! a built player reloads assets from the bundle, but that is
//! untested (research doc section 16).
//!
//! Ignored by default because it changes live game state:
//!
//! ```text
//! WWM_RADIUS=0.9 cargo test -p wwm-mod --test set_dig_radius -- --ignored --nocapture
//! ```
//!
//! Vanilla for the starting shovel is 0.45. Write that value
//! back, or restart the game, to undo.

mod common;
use common::{api, first_handle, handle_of, ping_or_skip};
use serde_json::json;

#[test]
#[ignore = "changes live game state on purpose"]
fn set_dig_radius() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let want: f64 = std::env::var("WWM_RADIUS")
        .expect("set WWM_RADIUS (vanilla starting shovel is 0.45)")
        .parse()
        .expect("WWM_RADIUS must be a number");

    let Some(ctrl) = first_handle(&api, "PlayerToolController") else {
        println!("PlayerToolController: no live instance; load a save first");
        return;
    };
    let tool = api.op("read_field", json!({"handle": ctrl, "field": "DigTool"}));
    let Some(th) = handle_of(&tool.result) else {
        println!("no equipped dig tool: {}", tool.result);
        return;
    };
    let name = tool.result["name"].as_str().unwrap_or("?").to_string();
    let data = api.op("invoke_method", json!({"handle": th, "method": "GetToolData", "args": []}));
    let Some(dh) = handle_of(&data.result) else {
        println!("no tool data: {}", data.result);
        return;
    };

    let before = api.op("read_field", json!({"handle": dh, "field": "radius"}));
    println!("{name}: radius before = {}", before.result);

    let write = api.op("write_field", json!({"handle": dh, "field": "radius", "value": want}));
    assert!(write.ok, "write_field failed: {:?}", write.error);

    let after = api.op("read_field", json!({"handle": dh, "field": "radius"}));
    println!("{name}: radius after  = {}", after.result);
    assert_eq!(after.result.as_f64(), Some(want), "radius did not take the new value");
}
