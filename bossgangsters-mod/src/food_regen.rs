//! Healing regen from fast food, scaled by price.
//!
//! Vanilla: eating calls `FastFoodItemData.ApplyEffects(fighter)`
//! which runs the item's effects (`FastFoodHealEffectSO` heals a
//! flat amount once). This hook adds a regen on top: 1 HP per
//! second for `price x 6` seconds, so the $100 item buys 10
//! minutes. Eating again ADDS its duration to what remains.
//!
//! Mechanics: a prefix on `ApplyEffects` (original always runs)
//! reads `price` off the item and takes ownership of the eater's
//! handle. A poller wakes once a second; when a buff is active it
//! queues a main-thread job that calls the game's own
//! `FighterHandler.Heal(1)`. The poller ends itself when the buff
//! runs out (cheap by construction), and a failed Heal (stale
//! fighter, session over) ends the buff.

use std::ffi::c_void;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use unityforge::hook::{HOOK_REGISTRY, patch_prefix_instance_args};
use unityforge::main_thread_queue::MAIN_QUEUE;
use unityforge::mono::{self, LogLevel, MonoObject, json_handle, owned_object};

/// Seconds of regen per $1 of food price ($100 -> 600 s).
const SECONDS_PER_DOLLAR: f64 = 6.0;

/// HP restored per tick, one tick per second.
const HP_PER_TICK: f64 = 1.0;

struct Buff {
    /// The eater. Owned handle; released when the buff ends.
    fighter: MonoObject,
    remaining_secs: f64,
}

/// The active buff. Only touched on the game's main thread (the
/// prefix and the queued tick both run there).
static BUFF: Mutex<Option<Buff>> = Mutex::new(None);

/// Mirror of "is a buff active" for the poller thread, so an
/// idle second costs one atomic load and no main-thread hop.
static BUFF_ACTIVE: AtomicBool = AtomicBool::new(false);

static POLLER: Mutex<Option<modforge::rpg::poller::PollerHandle>> = Mutex::new(None);

pub fn install() {
    match patch_prefix_instance_args("FastFoodItemData", "ApplyEffects", apply_effects_prefix) {
        Ok(hook) => {
            HOOK_REGISTRY.register(hook);
            mono::log(
                LogLevel::Info,
                "bossgangsters-mod: food regen armed (ApplyEffects patched)",
            );
        }
        Err(e) => mono::log(
            LogLevel::Error,
            &format!("bossgangsters-mod: food regen patch failed: {e}"),
        ),
    }
}

extern "C" fn apply_effects_prefix(instance: *const c_void, args_json: *const c_char) -> i32 {
    if let Err(e) = grant_regen(instance, args_json) {
        mono::log(
            LogLevel::Warn,
            &format!("bossgangsters-mod: food regen not granted: {e}"),
        );
    }
    0 // always run the vanilla effects
}

fn grant_regen(instance: *const c_void, args_json: *const c_char) -> Result<(), String> {
    if instance.is_null() || args_json.is_null() {
        return Err("null instance or args".into());
    }
    let item = owned_object(instance as i32);
    let args: serde_json::Value = {
        let raw = unsafe { std::ffi::CStr::from_ptr(args_json) }
            .to_str()
            .map_err(|e| format!("args not utf8: {e}"))?;
        serde_json::from_str(raw).map_err(|e| format!("args not json: {e}"))?
    };
    let fighter_handle =
        json_handle(args.get(0).ok_or("no fighter arg")?).ok_or("fighter arg has no handle")?;
    let fighter = owned_object(fighter_handle);

    let price = item
        .read_field("price")?
        .as_f64()
        .ok_or("price not a number")?;
    let added_secs = price * SECONDS_PER_DOLLAR;

    let total = {
        let mut buff = BUFF.lock();
        let carried = match buff.take() {
            Some(b) => b.remaining_secs, // old fighter handle dropped here
            None => 0.0,
        };
        let total = carried + added_secs;
        *buff = Some(Buff {
            fighter,
            remaining_secs: total,
        });
        total
    };
    BUFF_ACTIVE.store(true, Ordering::Release);
    ensure_poller();
    mono::log(
        LogLevel::Info,
        &format!(
            "bossgangsters-mod: food regen granted: ${price:.0} -> +{added_secs:.0}s, {total:.0}s total at {HP_PER_TICK} HP/s"
        ),
    );
    Ok(())
}

fn ensure_poller() {
    let mut poller = POLLER.lock();
    if poller.is_some() {
        return;
    }
    *poller = Some(modforge::rpg::poller::spawn_interval(
        "food_regen",
        Duration::from_secs(1),
        || {
            if !BUFF_ACTIVE.load(Ordering::Acquire) {
                // Buff over: this job is finished; end itself.
                if let Some(h) = POLLER.lock().take() {
                    h.stop_soon();
                }
                return;
            }
            MAIN_QUEUE.push(tick);
        },
    ));
}

/// One regen second, on the main thread.
fn tick() {
    let mut buff = BUFF.lock();
    let Some(b) = buff.as_mut() else {
        BUFF_ACTIVE.store(false, Ordering::Release);
        return;
    };
    match b.fighter.invoke("Heal", &serde_json::json!([HP_PER_TICK])) {
        Ok(_) => {
            b.remaining_secs -= 1.0;
            if b.remaining_secs <= 0.0 {
                mono::log(LogLevel::Info, "bossgangsters-mod: food regen ended");
                *buff = None;
                BUFF_ACTIVE.store(false, Ordering::Release);
            }
        }
        Err(e) => {
            mono::log(
                LogLevel::Warn,
                &format!("bossgangsters-mod: food regen ended (Heal failed: {e})"),
            );
            *buff = None;
            BUFF_ACTIVE.store(false, Ordering::Release);
        }
    }
}
