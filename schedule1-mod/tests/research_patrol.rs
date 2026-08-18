//! Research question 7 (docs/research.md): how vanilla patrol
//! routes work and whether we can reuse them for garrison goons.
//!
//! Probes:
//! a) list all FootPatrolRoute instances in the scene (names,
//!    waypoint count, waypoint positions)
//! b) list_methods on PatrolGroup and FootPatrolRoute to see the
//!    full API surface
//! c) list any live PatrolGroup instances to see how police use
//!    the system (members, current waypoint, route)
//!
//! ```text
//! cargo test -p schedule1-mod --test research_patrol -- --test-threads=1 --nocapture
//! ```

mod common;
use common::{api, first_handle, handle_of, parse_vec3, ping_or_skip, walk,
             print_declared_methods, count_of};
use serde_json::json;

#[test]
fn patrol_routes_in_scene() {
    let api = api();
    if ping_or_skip(&api).is_none() {
        return;
    }

    println!("=== FootPatrolRoute: class surface ===");
    print_declared_methods(&api, "ScheduleOne.NPCs.Behaviour.FootPatrolRoute");

    println!("\n=== PatrolGroup: class surface ===");
    print_declared_methods(&api, "ScheduleOne.NPCs.Behaviour.PatrolGroup");

    println!("\n=== FootPatrolRoute instances in scene ===");
    if let Some(routes) = walk(&api, "ScheduleOne.NPCs.Behaviour.FootPatrolRoute") {
        for (i, route) in routes.iter().enumerate() {
            let Some(rh) = route["handle"].as_i64() else {
                println!("route[{i}]: no handle");
                continue;
            };
            let name = route["name"].as_str().unwrap_or("?");
            println!("route[{i}] name={name}");

            let inspect = api.op("inspect_object", json!({"handle": rh}));
            println!("  inspect: {}",
                serde_json::to_string_pretty(&inspect.result).unwrap_or_default());

            // Read RouteName field
            let rn = api.op("read_field", json!({"handle": rh, "field": "RouteName"}));
            if rn.ok {
                println!("  RouteName = {}", rn.result);
            }

            // Read StartWaypointIndex
            let swi = api.op("read_field", json!({"handle": rh, "field": "StartWaypointIndex"}));
            if swi.ok {
                println!("  StartWaypointIndex = {}", swi.result);
            }

            // Read Waypoints array
            let wp = api.op("read_field", json!({"handle": rh, "field": "Waypoints"}));
            if wp.ok {
                if let Some(wph) = handle_of(&wp.result) {
                    if let Some(n) = count_of(&api, wph) {
                        println!("  Waypoints: {n} waypoint(s)");
                        for j in 0..n {
                            let item = api.op("invoke_method",
                                json!({"handle": wph, "method": "get_Item", "args": [j]}));
                            if item.ok {
                                if let Some(th) = handle_of(&item.result) {
                                    let pos = api.op("invoke_method",
                                        json!({"handle": th, "method": "get_position", "args": []}));
                                    if let Some(v) = parse_vec3(&pos.result) {
                                        println!("    wp[{j}] = ({:.1}, {:.1}, {:.1})", v.0, v.1, v.2);
                                    } else {
                                        println!("    wp[{j}] position = {}", pos.result);
                                    }
                                    api.op("release_handle", json!({"handle": th}));
                                } else {
                                    println!("    wp[{j}] = {}", item.result);
                                }
                            }
                        }
                    }
                    api.op("release_handle", json!({"handle": wph}));
                } else {
                    println!("  Waypoints = {}", wp.result);
                }
            }
            api.op("release_handle", json!({"handle": rh}));
        }
    }

    println!("\n=== Live PatrolGroup instances ===");
    if let Some(groups) = walk(&api, "ScheduleOne.NPCs.Behaviour.PatrolGroup") {
        for (i, group) in groups.iter().enumerate() {
            let Some(gh) = group["handle"].as_i64() else {
                println!("group[{i}]: no handle");
                continue;
            };
            let inspect = api.op("inspect_object", json!({"handle": gh}));
            println!("group[{i}]: {}",
                serde_json::to_string_pretty(&inspect.result).unwrap_or_default());

            let cw = api.op("read_field", json!({"handle": gh, "field": "CurrentWaypoint"}));
            if cw.ok {
                println!("  CurrentWaypoint = {}", cw.result);
            }

            let members = api.op("read_field", json!({"handle": gh, "field": "Members"}));
            if members.ok {
                if let Some(mh) = handle_of(&members.result) {
                    if let Some(n) = count_of(&api, mh) {
                        println!("  Members: {n}");
                    }
                    api.op("release_handle", json!({"handle": mh}));
                }
            }

            let route = api.op("read_field", json!({"handle": gh, "field": "Route"}));
            if route.ok {
                println!("  Route = {}", route.result);
            }
            api.op("release_handle", json!({"handle": gh}));
        }
    }

    // Full method lists (including inherited)
    println!("\n=== LawManager: all methods ===");
    let r = api.op("list_methods", json!({"class": "ScheduleOne.Law.LawManager"}));
    if r.ok {
        let methods = r.result["methods"].as_array().cloned().unwrap_or_default();
        for m in &methods {
            println!("  {}({}) -> {} [from: {}]{}",
                m["name"].as_str().unwrap_or("?"),
                m["params"].as_i64().unwrap_or(-1),
                m["return"].as_str().unwrap_or("?"),
                m["declared_on"].as_str().unwrap_or("?"),
                if m["static"].as_bool() == Some(true) { " [static]" } else { "" },
            );
        }
    }

    println!("\n=== PatrolGroup: all methods ===");
    let r = api.op("list_methods", json!({"class": "ScheduleOne.NPCs.Behaviour.PatrolGroup"}));
    if r.ok {
        let methods = r.result["methods"].as_array().cloned().unwrap_or_default();
        for m in &methods {
            println!("  {}({}) -> {} [from: {}]{}",
                m["name"].as_str().unwrap_or("?"),
                m["params"].as_i64().unwrap_or(-1),
                m["return"].as_str().unwrap_or("?"),
                m["declared_on"].as_str().unwrap_or("?"),
                if m["static"].as_bool() == Some(true) { " [static]" } else { "" },
            );
        }
    }

    // FootPatrolBehaviour is the key: it's the behavior component
    // that drives patrol. Inspect one to see its fields.
    println!("\n=== FootPatrolBehaviour instances + inspect ===");
    if let Some(fpbs) = walk(&api, "ScheduleOne.NPCs.Behaviour.FootPatrolBehaviour") {
        println!("  {} FootPatrolBehaviour(s) live", fpbs.len());
        if let Some(first) = fpbs.first() {
            if let Some(fh) = first["handle"].as_i64() {
                let inspect = api.op("inspect_object", json!({"handle": fh}));
                println!("  fpb[0] inspect:\n{}",
                    serde_json::to_string_pretty(&inspect.result).unwrap_or_default());
                api.op("release_handle", json!({"handle": fh}));
            }
        }
        for f in fpbs.iter().skip(1) {
            if let Some(h) = f["handle"].as_i64() {
                api.op("release_handle", json!({"handle": h}));
            }
        }
    }

    // Also inspect SentryBehaviour (stationary guard post)
    println!("\n=== SentryBehaviour instances + inspect ===");
    if let Some(sbs) = walk(&api, "ScheduleOne.NPCs.Behaviour.SentryBehaviour") {
        println!("  {} SentryBehaviour(s) live", sbs.len());
        if let Some(first) = sbs.first() {
            if let Some(sh) = first["handle"].as_i64() {
                let inspect = api.op("inspect_object", json!({"handle": sh}));
                println!("  sentry[0] inspect:\n{}",
                    serde_json::to_string_pretty(&inspect.result).unwrap_or_default());
                api.op("release_handle", json!({"handle": sh}));
            }
        }
        for s in sbs.iter().skip(1) {
            if let Some(h) = s["handle"].as_i64() {
                api.op("release_handle", json!({"handle": h}));
            }
        }
    }

    // Inspect the behavior stack on a minted NPC (our goons)
    // NPCBehaviour holds the behavior list
    println!("\n=== NPCBehaviour on first minted NPC ===");
    if let Some(behaviours) = walk(&api, "ScheduleOne.NPCs.Behaviour.NPCBehaviour") {
        println!("  {} NPCBehaviour(s) live", behaviours.len());
        // Look at the last one (likely our minted NPC, since
        // vanilla NPCs load first)
        if let Some(last) = behaviours.last() {
            if let Some(bh) = last["handle"].as_i64() {
                let inspect = api.op("inspect_object", json!({"handle": bh}));
                println!("  behaviour[last] inspect:\n{}",
                    serde_json::to_string_pretty(&inspect.result).unwrap_or_default());
                api.op("release_handle", json!({"handle": bh}));
            }
        }
        for b in behaviours.iter().rev().skip(1) {
            if let Some(h) = b["handle"].as_i64() {
                api.op("release_handle", json!({"handle": h}));
            }
        }
    }
}
