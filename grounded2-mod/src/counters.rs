// Bbp-specific perf counters. Generic primitives (`bump`,
// `time_scope`, `observe_peak`, `Ring`) live in `ueforge::counters`
// and `ueforge::winproc`; consumers reach for those directly.
//
// Snapshot endpoint diffs these statics across two `snapshot` calls
// to find cycle thieves.

use ueforge::{counter, counter_json};

// kill_hook (UHealthComponent vtable)
counter!(
    KILL_HOOK_FIRES,
    KILL_HOOK_PLAYER_FIRES,
    KILL_HOOK_OWNER_FULLNAME_ALLOCS,
    KILL_HOOK_LOG_LINES,
    KILL_HOOK_DAMAGETYPE_NAME_ALLOCS,
);

// fall_hook (player BP class vtable)
counter!(FALL_HOOK_FIRES, FALL_HOOK_FNNAME_ALLOCS);

// world_loader poller
counter!(WORLD_LOADER_POLLS, WORLD_LOADER_GOBJECTS_WALKS);

// ImGui RPG tab render path
counter!(IMGUI_TAB_RENDERS);

// HTTP endpoint
counter!(HTTP_REQUESTS);

// CPU time accumulators (nanoseconds spent inside hot paths). Diff
// over a window = total wall time g2rpg stole from the game thread.
counter!(
    TIME_NS_KILL_HOOK,
    TIME_NS_FALL_HOOK,
    TIME_NS_IMGUI_GET_XP,
    TIME_NS_HTTP_HANDLE,
);

/// Snapshot all counters as a JSON object. Called by debug::snapshot.
/// PE-queue and damage-ring counters are owned by their respective
/// container types (`GameThread`, `EventRing`) and surfaced through
/// dedicated keys below.
/// Stays here because the counter names measure Grounded 2's hooks and player-facing systems;
/// Ueforge owns the reusable counter and process-snapshot machinery.
pub fn snapshot_json() -> serde_json::Value {
    let mut obj = counter_json! {
        KILL_HOOK_FIRES                  => "kill_hook_fires",
        KILL_HOOK_PLAYER_FIRES           => "kill_hook_player_fires",
        KILL_HOOK_OWNER_FULLNAME_ALLOCS  => "kill_hook_owner_fullname_allocs",
        KILL_HOOK_LOG_LINES              => "kill_hook_log_lines",
        KILL_HOOK_DAMAGETYPE_NAME_ALLOCS => "kill_hook_damagetype_name_allocs",
        FALL_HOOK_FIRES                  => "fall_hook_fires",
        FALL_HOOK_FNNAME_ALLOCS          => "fall_hook_fnname_allocs",
        WORLD_LOADER_POLLS               => "world_loader_polls",
        WORLD_LOADER_GOBJECTS_WALKS      => "world_loader_gobjects_walks",
        IMGUI_TAB_RENDERS                => "imgui_tab_renders",
        HTTP_REQUESTS                    => "http_requests",
        TIME_NS_KILL_HOOK                => "time_ns_kill_hook",
        TIME_NS_FALL_HOOK                => "time_ns_fall_hook",
        TIME_NS_IMGUI_GET_XP             => "time_ns_imgui_get_xp",
        TIME_NS_HTTP_HANDLE              => "time_ns_http_handle",
    };
    if let Some(o) = obj.as_object_mut() {
        // PE queue (GameThread owns its counters):
        o.insert(
            "drain_pending_calls".into(),
            crate::debug::PE_QUEUE.drain_calls().into(),
        );
        o.insert(
            "drain_pending_drained_cmds".into(),
            crate::debug::PE_QUEUE.drained_cmds().into(),
        );
        o.insert("pe_queue_peak".into(), crate::debug::PE_QUEUE.peak().into());
        o.insert(
            "time_ns_drain_pending".into(),
            crate::debug::PE_QUEUE.time_ns().into(),
        );
        // Damage ring (EventRing owns its counters):
        o.insert(
            "damage_ring_pushes".into(),
            crate::debug::damage_ring_pushes().into(),
        );
        o.insert(
            "damage_ring_peak".into(),
            crate::debug::damage_ring_peak().into(),
        );
    }
    obj
}
