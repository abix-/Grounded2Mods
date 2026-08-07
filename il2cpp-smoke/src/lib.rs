//! IL2CPP smoke target.
//!
//! Phase 3b proof-point for the unityforge plan: demonstrates
//! the same Rust SDK + bridge ABI drives an IL2CPP game when
//! loaded through `Unityforge.Shim.Il2Cpp.dll`. Scope is
//! intentionally smaller than wwm-mod: a curl checklist
//! covering the four corner pieces of the bridge (ping,
//! walk_class, read_field + write_field round-trip, one
//! Harmony postfix).
//!
//! The target `(class, method)` for the Harmony postfix is
//! configured via the `IL2CPP_SMOKE_TARGET_CLASS` /
//! `IL2CPP_SMOKE_TARGET_METHOD` environment variables read at
//! init. Default targets resolve to `UnityEngine.Time` /
//! `get_realtimeSinceStartup` which is present on every
//! Unity game (Mono or IL2CPP). Override via the BepInEx
//! launcher's env file when smoke-testing a specific game.
//!
//! The bridge surface is the same on Mono and IL2CPP so this
//! crate also loads via the Mono shim if pointed at a Mono
//! game; it's named "il2cpp-smoke" because that's its
//! intended target.

use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value as Json, json};

use modforge::args::arg_str;
use modforge::ops::{OP_REGISTRY, OpDef};
use unityforge::ModDef;
use unityforge::hook::{HOOK_REGISTRY, patch_postfix};
use unityforge::unity::Type;

static MOD_INFO: ModDef = ModDef {
    name: "Il2CppSmoke",
    version: "0.1.0",
    // 17175: the modforge default 17173 is held by eufy-capture on
    // the operator's machine (bind failed with WSAEACCES 10013).
    http_port: 17175,
    on_init: Some(on_init),
    on_tick: None,
    on_shutdown: Some(on_shutdown),
    tabs: &[],
};

unityforge::unityforge_mod!(MOD_INFO);

static POSTFIX_FIRES: AtomicU64 = AtomicU64::new(0);
static SMOKE_TARGET: OnceLock<(String, String)> = OnceLock::new();

fn on_init() {
    unityforge::ops::register_builtins();
    unityforge::selector::register_builtins();
    register_smoke_ops();
    install_smoke_hook();

    let kind = unityforge::unity::runtime_kind()
        .map(|k| format!("{k:?}"))
        .unwrap_or_else(|| "<unset>".to_string());
    unityforge::mono::log(
        unityforge::mono::LogLevel::Info,
        &format!("il2cpp-smoke: ready (runtime={kind})"),
    );
}

fn on_shutdown() {
    unityforge::mono::log(
        unityforge::mono::LogLevel::Info,
        "il2cpp-smoke: shutdown",
    );
}

fn register_smoke_ops() {
    OP_REGISTRY.register_many([
        OpDef::new(
            "smoke_state",
            "Snapshot of the smoke target's runtime tag + postfix counter",
            "{}",
            |_args| {
                let kind = unityforge::unity::runtime_kind()
                    .map(|k| format!("{k:?}"))
                    .unwrap_or_else(|| "<unset>".to_string());
                let target = SMOKE_TARGET
                    .get()
                    .map(|(c, m)| format!("{c}::{m}"))
                    .unwrap_or_else(|| "<unset>".to_string());
                Ok(json!({
                    "runtime": kind,
                    "postfix_target": target,
                    "postfix_fires": POSTFIX_FIRES.load(Ordering::Relaxed),
                }))
            },
        ),
        OpDef::new(
            "smoke_read",
            "Read a field on a class's singleton instance",
            "{class: str, field: str}",
            |args| {
                let class = arg_str(args, "class")?;
                let field = arg_str(args, "field")?;
                let ty = Type::find(class).ok_or_else(|| format!("class '{class}' not found"))?;
                let obj = ty
                    .singleton_instance()
                    .ok_or_else(|| "no singleton instance".to_string())?;
                obj.read_field(field)
            },
        ),
        OpDef::new(
            "smoke_write",
            "Write a field on a class's singleton instance",
            "{class: str, field: str, value: <json>}",
            |args| {
                let class = arg_str(args, "class")?;
                let field = arg_str(args, "field")?;
                let value = args.get("value").cloned().unwrap_or(Json::Null);
                let ty = Type::find(class).ok_or_else(|| format!("class '{class}' not found"))?;
                let obj = ty
                    .singleton_instance()
                    .ok_or_else(|| "no singleton instance".to_string())?;
                obj.write_field(field, &value)?;
                Ok(json!({"class": class, "field": field, "written": true}))
            },
        ),
    ]);
}

fn install_smoke_hook() {
    let class = std::env::var("IL2CPP_SMOKE_TARGET_CLASS")
        .unwrap_or_else(|_| "UnityEngine.Time".to_string());
    let method = std::env::var("IL2CPP_SMOKE_TARGET_METHOD")
        .unwrap_or_else(|_| "get_realtimeSinceStartup".to_string());
    let _ = SMOKE_TARGET.set((class.clone(), method.clone()));
    // The shim's patch_postfix takes &str (static or borrowed).
    // The class/method strings are owned here; the shim copies
    // them into managed strings before calling Harmony, so the
    // borrow only needs to last across the patch_postfix call.
    if let Ok(hook) = patch_postfix(&class, &method, on_smoke_postfix) {
        HOOK_REGISTRY.register(hook);
        unityforge::mono::log(
            unityforge::mono::LogLevel::Info,
            &format!("il2cpp-smoke: patched {class}::{method}"),
        );
    } else {
        unityforge::mono::log(
            unityforge::mono::LogLevel::Warn,
            &format!("il2cpp-smoke: patch_postfix failed for {class}::{method}"),
        );
    }
}

extern "C" fn on_smoke_postfix(_ctx: *const c_void) {
    POSTFIX_FIRES.fetch_add(1, Ordering::Relaxed);
}
