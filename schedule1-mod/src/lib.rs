//! Schedule 1 mod. Rust cdylib loaded by Unityforge.Shim.Melon
//! (the MelonLoader entry) into the IL2CPP game.
//!
//! Bootstrap path:
//! 1. MelonLoader loads `Unityforge.Shim.Melon.dll` from Mods/.
//! 2. The shim locates `schedule1_mod.unityforge.dll` next to
//!    itself, `LoadLibrary`s it, and calls
//!    `unityforge_init(bridge)`.
//! 3. `on_init` registers the framework's generic ops and the
//!    Unity-side selectors: the HTTP control plane for live
//!    research (walk_class / inspect_object / read_field /
//!    write_field / invoke_method / list_singletons / ...).
//! 4. The shim's OnUpdate drives `unityforge_tick` every frame;
//!    hot reload swaps generations via `*.gen<N>.dll` drops.
//!
//! v1 is the control plane only. Gameplay (combat-XP levelling,
//! faction war) lands on top per docs/schedule1-plan.md, gated
//! by the research questions in docs/research.md.

use unityforge::ModDef;

mod combat_trace;
mod killcredit;
mod skills;

static MOD_INFO: ModDef = ModDef {
    name: "Schedule1Mod",
    version: "0.1.0",
    // 17175: the modforge default 17173 is held by eufy-capture
    // on the operator's machine. 17175 is the Schedule 1 port
    // (established by the il2cpp-smoke run this crate replaces).
    http_port: 17175,
    on_init: Some(on_init),
    on_tick: None,
    on_shutdown: Some(on_shutdown),
    tabs: &[],
};

unityforge::unityforge_mod!(MOD_INFO);

fn on_init() {
    unityforge::ops::register_builtins();
    unityforge::selector::register_builtins();
    combat_trace::register_ops();
    skills::install();
    killcredit::install();

    let kind = unityforge::unity::runtime_kind()
        .map(|k| format!("{k:?}"))
        .unwrap_or_else(|| "<unset>".to_string());
    unityforge::mono::log(
        unityforge::mono::LogLevel::Info,
        &format!("schedule1-mod: ready (runtime={kind})"),
    );
}

fn on_shutdown() {
    unityforge::mono::log(unityforge::mono::LogLevel::Info, "schedule1-mod: shutdown");
}
