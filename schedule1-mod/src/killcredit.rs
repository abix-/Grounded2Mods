//! XP on player kills: the attribution design proven by
//! tests/research_killcredit.rs (docs/research.md question 3).
//!
//! NotifyAttackedByPlayer fires on every player hit, in the same
//! frame as TakeDamage; melee-to-0 raises KnockOut, not Die. So:
//! - prefix on NPCHealth.NotifyAttackedByPlayer records
//!   (npc ptr, now) as a recent player hit.
//! - prefixes on NPCHealth.Die AND NPCHealth.KnockOut credit XP
//!   when the downed NPC took a player hit within the window,
//!   deduped per NPC so KnockOut-then-Die pays once.
//!
//! All three callbacks run inside Harmony prefixes on the game's
//! main thread; reads are safe there.

use std::ffi::c_void;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::Value as Json;

use unityforge::bridge::MonoHandle;
use unityforge::hook::{self, HOOK_REGISTRY, HookCtx};
use unityforge::mono::{self, LogLevel, MonoObject};

use crate::skills::TRACKER;

/// A hit stays creditable this long.
const HIT_WINDOW: Duration = Duration::from_secs(15);
/// One credit per NPC per this long (KnockOut then Die = one).
const CREDIT_COOLDOWN: Duration = Duration::from_secs(60);
const XP_PER_DOWN: u64 = 25;

static PLAYER_HITS: Mutex<Vec<(i64, Instant)>> = Mutex::new(Vec::new());
static CREDITED: Mutex<Vec<(i64, Instant)>> = Mutex::new(Vec::new());

pub fn install() {
    let targets: [(&str, extern "C" fn(*const c_void) -> i32); 3] = [
        ("NotifyAttackedByPlayer", on_player_hit),
        ("Die", on_down),
        ("KnockOut", on_down),
    ];
    for (method, cb) in targets {
        match hook::patch_prefix_ctx(
            "Il2CppScheduleOne.NPCs.NPCHealth",
            method,
            HookCtx::Instance,
            cb,
        ) {
            Ok(h) => HOOK_REGISTRY.register(h),
            Err(e) => mono::log(
                LogLevel::Error,
                &format!("schedule1-mod: killcredit patch NPCHealth.{method} FAILED: {e}"),
            ),
        }
    }
    mono::log(
        LogLevel::Info,
        "schedule1-mod: killcredit hooks installed (NotifyAttackedByPlayer + Die + KnockOut)",
    );
}

use crate::loot::parse_vec3;

/// The NPC's stable native pointer from an NPCHealth ctx handle
/// (releases every handle it takes).
fn npc_ptr(health_h: i32) -> Option<i64> {
    npc_info(health_h).map(|(ptr, _, _)| ptr)
}

/// Pointer, world position, and max health of the NPC behind an
/// NPCHealth ctx handle (releases every handle it takes).
fn npc_info(health_h: i32) -> Option<(i64, Option<(f64, f64, f64)>, f32)> {
    if health_h == 0 {
        return None;
    }
    // SAFETY: the shim acquired this handle for this callback and
    // we own it; Drop releases it.
    let obj = unsafe { MonoObject::from_handle(MonoHandle(health_h)) };
    let max_health = obj
        .read_field("MaxHealth")
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(100.0) as f32;
    let v = obj.read_field("npc").ok()?;
    let ptr = v.get("ptr").and_then(Json::as_i64)?;
    let mut pos = None;
    if let Some(h) = v.get("handle").and_then(Json::as_i64) {
        // SAFETY: chained handle from the read above; we own it.
        let npc = unsafe { MonoObject::from_handle(MonoHandle(h as i32)) };
        if let Ok(t) = npc.read_field("transform") {
            if let Some(th) = t.get("handle").and_then(Json::as_i64) {
                // SAFETY: chained handle, same ownership rule.
                let transform = unsafe { MonoObject::from_handle(MonoHandle(th as i32)) };
                pos = transform
                    .invoke("get_position", &Json::Array(vec![]))
                    .ok()
                    .and_then(|p| parse_vec3(&p));
            }
        }
    }
    Some((ptr, pos, max_health))
}

fn remember(list: &Mutex<Vec<(i64, Instant)>>, ptr: i64, keep: Duration) {
    let now = Instant::now();
    let mut l = list.lock();
    l.retain(|(p, t)| *p != ptr && now.duration_since(*t) < keep);
    l.push((ptr, now));
}

fn recent(list: &Mutex<Vec<(i64, Instant)>>, ptr: i64, window: Duration) -> bool {
    let now = Instant::now();
    list.lock()
        .iter()
        .any(|(p, t)| *p == ptr && now.duration_since(*t) < window)
}

/// Inert until the loaded game settles (crash guard: never touch
/// half-initialized instances during a save load). The ctx
/// handle must still be released or the table leaks.
fn release_only(health_h: i32) {
    if health_h != 0 {
        // SAFETY: the shim acquired this handle for this callback
        // and we own it; Drop releases it.
        drop(unsafe { MonoObject::from_handle(MonoHandle(health_h)) });
    }
}

fn settled() -> bool {
    crate::skills::SETTLED.load(std::sync::atomic::Ordering::Relaxed)
}

extern "C" fn on_player_hit(ctx: *const c_void) -> i32 {
    if !settled() {
        release_only(ctx as isize as i32);
        return 0;
    }
    if let Some(ptr) = npc_ptr(ctx as isize as i32) {
        remember(&PLAYER_HITS, ptr, HIT_WINDOW);
    }
    0
}

extern "C" fn on_down(ctx: *const c_void) -> i32 {
    if !settled() {
        release_only(ctx as isize as i32);
        return 0;
    }
    let Some((ptr, pos, max_health)) = npc_info(ctx as isize as i32) else {
        return 0;
    };
    if !recent(&PLAYER_HITS, ptr, HIT_WINDOW) {
        return 0; // not the player's kill
    }
    if recent(&CREDITED, ptr, CREDIT_COOLDOWN) {
        return 0; // already paid for this down
    }
    remember(&CREDITED, ptr, CREDIT_COOLDOWN);
    // Farm mobs carry rolled XP + loot multipliers; anything
    // else pays base.
    let (xp_mult, loot_mult) = match crate::farming::on_mob_down(ptr) {
        Some((xm, lm, label)) => {
            mono::log(LogLevel::Info, &format!("schedule1-mod: {label} is down"));
            (xm, lm)
        }
        None => (1.0, 1.0),
    };
    if let Some((x, y, z)) = pos {
        crate::loot::drop_cash_at(x, y, z, max_health * loot_mult);
    }
    if let Some(r) = TRACKER.record_xp((XP_PER_DOWN as f32 * xp_mult) as u64) {
        let lvl = if r.new_level > r.old_level {
            format!(" LEVEL UP -> {} (+{} point(s))", r.new_level, r.points_gained)
        } else {
            String::new()
        };
        mono::log(
            LogLevel::Info,
            &format!("schedule1-mod: +{} XP for the kill (total {}){lvl}", r.awarded, r.total_xp),
        );
        if r.points_gained > 0 {
            crate::skills::auto_spend(r.points_gained);
        }
    }
    0
}
