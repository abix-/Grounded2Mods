//! What does the 1.0 release's native skill system actually
//! contain?
//!
//! `SkillsManager` survived the release intact (research doc
//! section 12.3) and is the intended backing for wwm-mod's
//! Strong Back / Resilient / Charisma once they stop writing
//! raw fields. Before writing that Effect we need the skill ids
//! the game defines, the level ceiling per skill, and what a
//! level is worth.
//!
//! Probes:
//! a) SkillsManager.skillsData (SkillsDataSO): the authored
//!    catalog
//! b) SkillsManager.database (SkillsDatabase): the live per-save
//!    state
//! c) the declared surface of both types
//!
//! ```text
//! cargo test -p wwm-mod --test research_skills -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, first_handle, handle_of, ping_or_skip, print_declared_methods};
use serde_json::json;

#[test]
fn skills_data_and_database() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let Some(mgr) = first_handle(&api, "SkillsManager") else {
        println!("SkillsManager: no live instance; load a save first");
        return;
    };

    for field in ["skillsData", "database"] {
        println!("\n=== SkillsManager.{field} ===");
        let read = api.op("read_field", json!({"handle": mgr, "field": field}));
        if !read.ok {
            println!("read_field failed: {:?}", read.error);
            continue;
        }
        let Some(h) = handle_of(&read.result) else {
            println!("no handle on {}", read.result);
            continue;
        };
        let ty = read.result["type"].as_str().unwrap_or("?").to_string();
        println!("type: {ty}");

        let inspect = api.op("inspect_object", json!({"handle": h}));
        println!("fields: {}", inspect.result);

        println!("declared methods:");
        print_declared_methods(&api, &ty);
    }
}

/// The authored catalog: every SkillDataSO in
/// SkillsDataSO.skillDatas, with its id, level ceiling and the
/// values a level buys.
#[test]
fn skill_catalog_entries() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let Some(mgr) = first_handle(&api, "SkillsManager") else {
        println!("SkillsManager: no live instance; load a save first");
        return;
    };
    let data = api.op("read_field", json!({"handle": mgr, "field": "skillsData"}));
    let Some(dh) = handle_of(&data.result) else {
        println!("no skillsData handle: {}", data.result);
        return;
    };
    let list = api.op("read_field", json!({"handle": dh, "field": "skillDatas"}));
    let Some(lh) = handle_of(&list.result) else {
        println!("no skillDatas handle: {}", list.result);
        return;
    };

    let count = api.op("invoke_method", json!({"handle": lh, "method": "get_Count", "args": []}));
    println!("skillDatas count: {} (ok={})", count.result, count.ok);
    let n = count.result.as_i64().unwrap_or(0);

    for i in 0..n {
        let item = api.op(
            "invoke_method",
            json!({"handle": lh, "method": "get_Item", "args": [i]}),
        );
        let Some(ih) = handle_of(&item.result) else {
            println!("[{i}] no handle: {} ok={}", item.result, item.ok);
            continue;
        };
        let inspect = api.op("inspect_object", json!({"handle": ih}));
        println!("\n[{i}] {}", inspect.result);

        // Per-level entries: what a level of this skill buys and
        // what it costs.
        let levels = api.op("read_field", json!({"handle": ih, "field": "skillDatas"}));
        let Some(llh) = handle_of(&levels.result) else {
            continue;
        };
        let lc = api.op("invoke_method", json!({"handle": llh, "method": "get_Count", "args": []}));
        let ln = lc.result.as_i64().unwrap_or(0);
        println!("  levels: {ln}");
        for j in 0..ln {
            let lvl = api.op(
                "invoke_method",
                json!({"handle": llh, "method": "get_Item", "args": [j]}),
            );
            match handle_of(&lvl.result) {
                Some(lh2) => {
                    let li = api.op("inspect_object", json!({"handle": lh2}));
                    println!("  [{j}] {}", li.result);
                }
                None => println!("  [{j}] {}", lvl.result),
            }
        }
    }
}

/// Can the control plane drive SkillsManager by enum name, and
/// what does each skill read right now? A proxy Effect needs
/// both answers: it will call SetSkillLevel(skillType, level)
/// from Rust with the skill named as a string.
#[test]
fn read_current_levels() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let Some(mgr) = first_handle(&api, "SkillsManager") else {
        println!("SkillsManager: no live instance; load a save first");
        return;
    };

    for skill in ["Bag", "Energy", "Rope", "Speed"] {
        let level = api.op(
            "invoke_method",
            json!({"handle": mgr, "method": "GetCurrentSkillLevel", "args": [skill]}),
        );
        let levels = api.op(
            "invoke_method",
            json!({"handle": mgr, "method": "GetSkillLevelsAmount", "args": [skill]}),
        );
        let v1 = api.op(
            "invoke_method",
            json!({"handle": mgr, "method": "GetCurrentSkillValue", "args": [skill, false]}),
        );
        let v2 = api.op(
            "invoke_method",
            json!({"handle": mgr, "method": "GetCurrentSkillValue", "args": [skill, true]}),
        );
        println!(
            "{skill:<8} level={} (ok={})  of={}  value={}  value2={}",
            level.result, level.ok, levels.result, v1.result, v2.result
        );
        if !level.ok {
            println!("         error: {:?}", level.error);
        }
    }
}

/// The live per-save state behind SkillsDatabase.SkillsData.
#[test]
fn skill_save_state() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    let Some(mgr) = first_handle(&api, "SkillsManager") else {
        println!("SkillsManager: no live instance; load a save first");
        return;
    };
    let db = api.op("read_field", json!({"handle": mgr, "field": "database"}));
    let Some(dbh) = handle_of(&db.result) else {
        println!("no database handle: {}", db.result);
        return;
    };
    let state = api.op("read_field", json!({"handle": dbh, "field": "skillsData"}));
    let Some(sh) = handle_of(&state.result) else {
        println!("no skillsData handle: {}", state.result);
        return;
    };
    let inspect = api.op("inspect_object", json!({"handle": sh}));
    println!("SkillsData: {}", inspect.result);
    println!("declared methods:");
    print_declared_methods(&api, "SkillsData");
}
