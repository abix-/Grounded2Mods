//! What controls how much ground one swing removes?
//!
//! `DigManager.DigShovel()` calls
//! `VoxelWorld.ModifyDensity(point, radius, strength, isFill:
//! false)` where
//!   radius   = PlayerToolController.DigTool.GetToolData().GetRadius()
//!              divided by a depth factor
//!   strength = DiggingSettingsManager.DiggingSettings.ShovelDigStrength
//!
//! So "dig strength" is two separate numbers: a global one on a
//! ScriptableObject, and a per-tool one that the shovel upgrades
//! move. This reads both live.
//!
//! ```text
//! cargo test -p wwm-mod --test research_digging -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, first_handle, handle_of, ping_or_skip, print_declared_methods};
use serde_json::json;

#[test]
fn global_digging_settings() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let Some(mgr) = first_handle(&api, "DiggingSettingsManager") else {
        println!("DiggingSettingsManager: no live instance");
        return;
    };
    let settings = api.op("read_field", json!({"handle": mgr, "field": "diggingSettings"}));
    let Some(sh) = handle_of(&settings.result) else {
        println!("no diggingSettings handle: {}", settings.result);
        return;
    };
    let inspect = api.op("inspect_object", json!({"handle": sh}));
    println!("DiggingSettingsSO: {}", inspect.result);
}

#[test]
fn current_dig_tool() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let Some(ctrl) = first_handle(&api, "PlayerToolController") else {
        println!("PlayerToolController: no live instance");
        return;
    };
    println!("PlayerToolController surface:");
    print_declared_methods(&api, "PlayerToolController");

    let tool = api.op("read_field", json!({"handle": ctrl, "field": "DigTool"}));
    println!("DigTool: ok={} {}", tool.ok, tool.result);
    let Some(th) = handle_of(&tool.result) else {
        return;
    };
    let data = api.op("invoke_method", json!({"handle": th, "method": "GetToolData", "args": []}));
    let Some(dh) = handle_of(&data.result) else {
        println!("no tool data handle: {}", data.result);
        return;
    };
    let inspect = api.op("inspect_object", json!({"handle": dh}));
    println!("ToolDataSO: {}", inspect.result);
}
