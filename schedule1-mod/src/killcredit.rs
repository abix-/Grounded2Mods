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
use std::time::Duration;

use serde_json::Value as Json;

use modforge::client::parse_vec3;
use modforge::ring::RecentRing;
use unityforge::hook::{self, HOOK_REGISTRY, HookCtx};
use unityforge::mono::{self, LogLevel};

use crate::skills::TRACKER;

/// A hit stays creditable this long.
const HIT_WINDOW: Duration = Duration::from_secs(15);
/// One credit per NPC per this long (KnockOut then Die = one).
const CREDIT_COOLDOWN: Duration = Duration::from_secs(60);
const XP_PER_DOWN: u64 = 25;
const RING_CAP: usize = 32;

static PLAYER_HITS: RecentRing<i64, RING_CAP> = RecentRing::new();
static CREDITED: RecentRing<i64, RING_CAP> = RecentRing::new();

/// Hooks Schedule 1's player-hit, death, and knockout signals used for kill rewards.
/// Stays here because the target class and signal combination are game facts; Unityforge owns Harmony hook installation.
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

/// The NPC's stable native pointer from an NPCHealth ctx handle
/// (releases every handle it takes).
/// Stays here because Schedule 1's NPCHealth-to-NPC field path defines the attribution identity; Unityforge owns managed reads.
fn npc_ptr(health_h: i32) -> Option<i64> {
    npc_info(health_h).map(|(ptr, _, _)| ptr)
}

/// Pointer, world position, and max health of the NPC behind an
/// NPCHealth ctx handle (releases every handle it takes).
/// Stays here because these exact fields feed Schedule 1's rewards and war; Unityforge owns object invocation and handle lifetime.
fn npc_info(health_h: i32) -> Option<(i64, Option<(f64, f64, f64)>, f32)> {
    if health_h == 0 {
        return None;
    }
    let obj = mono::owned_object(health_h);
    let max_health = obj
        .read_field("MaxHealth")
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(100.0) as f32;
    let v = obj.read_field("npc").ok()?;
    let ptr = v.get("ptr").and_then(Json::as_i64)?;
    let mut pos = None;
    if let Some(h) = mono::json_handle(&v) {
        let npc = mono::owned_object(h);
        if let Ok(t) = npc.read_field("transform") {
            if let Some(th) = mono::json_handle(&t) {
                let transform = mono::owned_object(th);
                pos = transform
                    .invoke("get_position", &Json::Array(vec![]))
                    .ok()
                    .and_then(|p| parse_vec3(&p));
            }
        }
    }
    Some((ptr, pos, max_health))
}


/// Inert until the loaded game settles (crash guard: never touch
/// half-initialized instances during a save load). The ctx
/// handle must still be released or the table leaks.
/// Stays here because the settling policy protects this mod's Schedule 1 callbacks; Unityforge owns the managed handle wrapper.
fn release_only(health_h: i32) {
    if health_h != 0 {
        drop(mono::owned_object(health_h));
    }
}

/// Reports whether Schedule 1's current save is old enough for game-object access.
/// Stays here because it reads this mod's load guard; a framework cannot choose the game's safe settling window.
fn settled() -> bool {
    crate::skills::SETTLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Marks an NPC as recently damaged by the player when Schedule 1 emits its player-hit signal.
/// Stays here because this callback implements the game's verified attribution rule; Unityforge owns callback plumbing.
extern "C" fn on_player_hit(ctx: *const c_void) -> i32 {
    if !settled() {
        release_only(ctx as isize as i32);
        return 0;
    }
    if let Some(ptr) = npc_ptr(ctx as isize as i32) {
        PLAYER_HITS.remember(ptr);
    }
    0
}

/// Pays XP and loot once when a recently player-hit Schedule 1 NPC is knocked out or dies.
/// Stays here because reward values, war integration, and knockout semantics are game behavior; the frameworks supply primitives.
extern "C" fn on_down(ctx: *const c_void) -> i32 {
    if !settled() {
        release_only(ctx as isize as i32);
        return 0;
    }
    let Some((ptr, pos, max_health)) = npc_info(ctx as isize as i32) else {
        return 0;
    };
    let player_hit = PLAYER_HITS.recent(ptr, HIT_WINDOW);
    let already_credited = CREDITED.recent(ptr, CREDIT_COOLDOWN);
    mono::log(
        LogLevel::Info,
        &format!(
            "schedule1-mod [kill]: npc down ptr={ptr} player_hit={player_hit} already_credited={already_credited} max_health={max_health:.0} pos={pos:?}"
        ),
    );
    if !player_hit {
        return 0;
    }
    if already_credited {
        return 0;
    }
    CREDITED.remember(ptr);
    let (xp_mult, loot_mult) = match crate::farming::on_mob_down(ptr) {
        Some((xm, lm, label)) => {
            mono::log(LogLevel::Info, &format!("schedule1-mod: {label} is down"));
            (xm, lm)
        }
        None => {
            mono::log(
                LogLevel::Info,
                &format!("schedule1-mod [kill]: ptr={ptr} not in garrison forces (vanilla NPC?)"),
            );
            (1.0, 1.0)
        }
    };
    if let Some((x, y, z)) = pos {
        mono::log(
            LogLevel::Info,
            &format!("schedule1-mod [kill]: dropping loot at ({x:.0},{y:.0},{z:.0}) max_health={max_health:.0} loot_mult={loot_mult:.2}"),
        );
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
