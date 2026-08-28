//! Spacebar -> jump.
//!
//! WWM ships no native jump. This module adds one by polling
//! `Input.GetKeyDown(KeyCode.Space)` (via the unityforge input
//! binding) and `Transform.Translate(0, JUMP_HEIGHT, 0)` on
//! the player Transform when grounded. Gravity (the player's
//! Rigidbody has useGravity=true) handles the descent.
//!
//! Side effect: also surfaces a `jump` HTTP op so the curl
//! surface can trigger the same jump without keypress (useful
//! for testing).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;

use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::input::{KeyCode, register_key_press};
use unityforge::mono::{LogLevel, MonoType, log};

/// Vertical impulse magnitude. mass=70 -> dv = JUMP_IMPULSE/70
/// m/s upward. 500 -> ~7 m/s, peaks ~2.5 m up at gravity 9.8
/// or higher with the game's slow fallSpeed=0.4.
const JUMP_IMPULSE: f32 = 500.0;

static PLAYER_TYPE: OnceLock<MonoType> = OnceLock::new();
static JUMP_COUNT: AtomicU64 = AtomicU64::new(0);

pub fn install() {
    OP_REGISTRY.register(OpDef::new(
        "jump",
        "Trigger a jump on the player (Translate +3m on Y)",
        "{}",
        |_args| match do_jump() {
            Ok(()) => Ok(json!({
                "jumped": true,
                "total_jumps": JUMP_COUNT.load(Ordering::Relaxed),
            })),
            Err(e) => Err(e),
        },
    ));

    if let Some(handle) = register_key_press(KeyCode::Space, on_space_pressed) {
        log(
            LogLevel::Info,
            &format!("wwm-mod/jump: spacebar bound (handle {handle})"),
        );
    } else {
        log(
            LogLevel::Warn,
            "wwm-mod/jump: register_key_press returned None (bridge not installed?)",
        );
    }
}

extern "C" fn on_space_pressed() {
    if let Err(e) = do_jump() {
        log(LogLevel::Warn, &format!("wwm-mod/jump: {e}"));
    }
}

fn do_jump() -> Result<(), String> {
    let ty = match PLAYER_TYPE.get() {
        Some(t) => t,
        None => {
            let t = MonoType::find("PlayerController")
                .ok_or_else(|| "PlayerController type not loaded".to_string())?;
            PLAYER_TYPE.get_or_init(|| t)
        }
    };
    let walk = ty
        .walk(false)
        .map_err(|e| format!("walk PlayerController: {e}"))?;
    let instances = walk
        .get("instances")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "walk: missing instances".to_string())?;
    let first = instances
        .first()
        .ok_or_else(|| "no PlayerController instance live (load a save first)".to_string())?;
    let handle = first
        .get("handle")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| "instance has no handle".to_string())? as i32;
    // SAFETY: handle from walk_class is live for this call.
    let player = unsafe {
        unityforge::mono::MonoObject::from_handle(unityforge::bridge::MonoHandle(handle))
    };

    // Get the Rigidbody handle so we can apply a vertical
    // impulse + the player handle so we can clear isGrounded
    // (the game's grounded-clamp logic re-pins us otherwise).
    let rb_handle = player
        .read_field("rb")
        .map_err(|e| format!("read rb: {e}"))?
        .get("handle")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| "rb has no handle".to_string())? as i32;
    // SAFETY: handle from a single read_field call, live for
    // this method's duration.
    let rb = unsafe {
        unityforge::mono::MonoObject::from_handle(unityforge::bridge::MonoHandle(rb_handle))
    };

    // Lift isGrounded so PlayerController's per-frame
    // grounded-clamp doesn't pin us to the floor mid-arc.
    // Game will re-set it true on its own ground-check raycast
    // when we land.
    let _ = player.write_field("isGrounded", &json!(false));

    // Apply upward impulse. ForceMode.Impulse = 1; this
    // overload takes (x, y, z, mode) all as separate primitives
    // so we don't need Vector3 deserialization in the shim.
    // mass = 70, so dv = JUMP_IMPULSE / 70 m/s.
    rb.invoke("AddForce", &json!([0.0, JUMP_IMPULSE, 0.0, 1]))
        .map_err(|e| format!("AddForce: {e}"))?;

    JUMP_COUNT.fetch_add(1, Ordering::Relaxed);
    Ok(())
}
