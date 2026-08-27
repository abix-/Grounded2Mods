//! Research diagnostic: kill attribution (docs/research.md
//! question 3). Records every firing of the NPCHealth combat
//! entry points so a live fight shows which signals exist for
//! crediting the player with a kill.
//!
//! TakeDamage(Single, Boolean, Boolean) carries no attacker, so
//! attribution has to come from NotifyAttackedByPlayer(int) or
//! elsewhere; this trace proves live whether that call fires
//! when the player attacks, and whether Die on the same NPC
//! follows it.
//!
//! Ops (invoked by tests/research_killcredit.rs, never curl):
//! - combat_trace_start: prefix-hook TakeDamage,
//!   NotifyAttackedByPlayer, Die, KnockOut on NPCHealth; each
//!   firing records {event, ms, npc, npc_ptr, health}.
//! - combat_trace_report {clear?}: the recorded events.
//! - combat_trace_stop: drop the hooks (events kept).

use std::ffi::c_void;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::{Value as Json, json};

use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::hook::{self, Hook, HookCtx};
use unityforge::main_thread_queue::MAIN_QUEUE;
use unityforge::mono;

const CLASS: &str = "Il2CppScheduleOne.NPCs.NPCHealth";
const MAX_EVENTS: usize = 512;

static EVENTS: Mutex<Vec<Json>> = Mutex::new(Vec::new());
static HOOKS: Mutex<Vec<Hook>> = Mutex::new(Vec::new());
static STARTED: Mutex<Option<Instant>> = Mutex::new(None);

/// Registers Schedule 1's start, report, and stop operations for live combat research.
/// Stays here because the operation set investigates this game's kill signals; Modforge owns the operation registry.
pub fn register_ops() {
    OP_REGISTRY.register_many([
        OpDef::new(
            "combat_trace_start",
            "Install recording prefix hooks on NPCHealth TakeDamage / NotifyAttackedByPlayer / Die / KnockOut. Kill-attribution research; report per-method ok or failure.",
            "{}",
            combat_trace_start,
        ),
        OpDef::new(
            "combat_trace_report",
            "Events recorded since combat_trace_start: {event, ms, npc, npc_ptr, health}.",
            "{clear?: bool}",
            combat_trace_report,
        ),
        OpDef::new(
            "combat_trace_stop",
            "Drop the combat-trace hooks (recorded events kept until a cleared report).",
            "{}",
            combat_trace_stop,
        ),
    ]);
}

/// Record one firing. Runs inside the Harmony prefix on the
/// game's thread: read the NPC identity off the NPCHealth
/// instance, then release every handle taken.
/// Stays here because the event fields and NPCHealth layout answer a Schedule 1 research question; Unityforge owns managed access.
fn record(event: &str, health_h: i32) {
    let started = *STARTED.lock();
    let ms = started.map_or(0, |t| t.elapsed().as_millis() as u64);
    let (mut npc, mut npc_ptr, mut health) = (Json::Null, Json::Null, Json::Null);
    if health_h != 0 {
        let obj = mono::owned_object(health_h);
        if let Ok(v) = obj.read_field("npc") {
            npc = v.get("str").cloned().unwrap_or(Json::Null);
            npc_ptr = v.get("ptr").cloned().unwrap_or(Json::Null);
            if let Some(h) = mono::json_handle(&v) {
                drop(mono::owned_object(h));
            }
        }
        if let Ok(v) = obj.read_field("Health") {
            health = v;
        }
        drop(obj);
    }
    let mut events = EVENTS.lock();
    if events.len() >= MAX_EVENTS {
        events.remove(0);
    }
    events.push(json!({
        "event": event, "ms": ms,
        "npc": npc, "npc_ptr": npc_ptr, "health": health,
    }));
}

/// Records Schedule 1's raw damage signal for the current NPC.
/// Stays here because it labels a game-specific hook target; Unityforge owns callback context delivery.
extern "C" fn on_take_damage(ctx: *const c_void) -> i32 {
    record("TakeDamage", ctx as isize as i32);
    0
}

/// Records Schedule 1's explicit player-attack notification for kill-attribution research.
/// Stays here because the signal is a verified game fact; Unityforge owns callback context delivery.
extern "C" fn on_notify_attacked_by_player(ctx: *const c_void) -> i32 {
    record("NotifyAttackedByPlayer", ctx as isize as i32);
    0
}

/// Records Schedule 1's NPC death signal for comparison with other combat events.
/// Stays here because the trace labels this game's event; Unityforge owns callback context delivery.
extern "C" fn on_die(ctx: *const c_void) -> i32 {
    record("Die", ctx as isize as i32);
    0
}

/// Records Schedule 1's NPC knockout signal, which melee defeats use instead of death.
/// Stays here because that distinction is game behavior; Unityforge owns callback context delivery.
extern "C" fn on_knock_out(ctx: *const c_void) -> i32 {
    record("KnockOut", ctx as isize as i32);
    0
}

/// Installs the four Schedule 1 NPCHealth trace hooks and reports which targets resolved.
/// Stays here because the class, methods, and event labels are game-specific; Unityforge owns main-thread hook installation.
fn combat_trace_start(_args: &Json) -> Result<Json, String> {
    // Patching goes through Harmony on the game's main thread,
    // same as every game-touching op.
    MAIN_QUEUE.run("combat_trace_start", Duration::from_secs(5), || {
        let mut hooks = HOOKS.lock();
        if !hooks.is_empty() {
            return Err("combat trace already running; combat_trace_stop first".into());
        }
        *STARTED.lock() = Some(Instant::now());
        let targets: [(&str, extern "C" fn(*const c_void) -> i32); 4] = [
            ("TakeDamage", on_take_damage),
            ("NotifyAttackedByPlayer", on_notify_attacked_by_player),
            ("Die", on_die),
            ("KnockOut", on_knock_out),
        ];
        let mut report = Vec::new();
        for (method, cb) in targets {
            match hook::patch_prefix_ctx(CLASS, method, HookCtx::Instance, cb) {
                Ok(h) => {
                    hooks.push(h);
                    report.push(json!({"method": method, "patched": true}));
                }
                Err(e) => report.push(json!({"method": method, "patched": false, "error": e})),
            }
        }
        Ok(json!({"hooks": report}))
    })?
}

/// Returns the bounded Schedule 1 combat trace and optionally clears its recorded events.
/// Stays here because the response is this mod's research presentation; Modforge owns only generic operation transport.
fn combat_trace_report(args: &Json) -> Result<Json, String> {
    let clear = args.get("clear").and_then(Json::as_bool).unwrap_or(false);
    let mut events = EVENTS.lock();
    let out = json!({"tracing": !HOOKS.lock().is_empty(), "events": *events});
    if clear {
        events.clear();
    }
    Ok(out)
}

/// Removes Schedule 1's temporary combat hooks while retaining their observations.
/// Stays here because it controls this mod's research session; Unityforge owns hook teardown through `Hook` lifetime.
fn combat_trace_stop(_args: &Json) -> Result<Json, String> {
    // Unpatching (Hook::drop) is a Harmony call: main thread.
    MAIN_QUEUE.run("combat_trace_stop", Duration::from_secs(5), || {
        let mut hooks = HOOKS.lock();
        let n = hooks.len();
        hooks.clear();
        Ok(json!({"dropped": n}))
    })?
}
