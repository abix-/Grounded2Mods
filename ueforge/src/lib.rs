//! ueforge: base layer for every UE4SS Rust mod in this workspace.
//!
//! One crate, no feature flags. ueforge's own `build.rs` compiles
//! the always-safe C++ (imgui vendor + `ueforge_ui.cpp`) into a
//! static lib that cargo links into every consumer of the crate
//!. Cdylib, test binaries, the game's build-script binary alike.
//!
//! Mods that want the shipped UE4SS factory (`start_mod` /
//! `uninstall_mod` / `DllMain` + the `UespyMod` CppUserModBase
//! subclass) call [`build::CppShim::new`]`.compile()` from their
//! own `build.rs`. Mods with their own shim (grounded2-mod
//! today) chain `.skip_default_shim()`.
//!
//! Operating principle: always change ueforge first. Game-specific
//! code goes on top; if it might apply to other UE games, it
//! belongs here.
//!
//! ## The five framework modules
//!
//! ueforge ships opinionated modules for the most common UE5 mod
//! patterns. A new game's mod picks from these menus and writes
//! only game-specific knobs:
//!
//! - **[`rpg`]**. Skill catalog + XP curve + bestiary +
//!   per-slot persistence + ImGui tab + the `StandardEffect`
//!   variant menu covering 9 of the 10 most common skill shapes.
//! - **[`stacks`]**. Inventory stack-size data-table tweak with
//!   captured-vanilla baseline, idempotent re-apply, multiplier
//!   atomic, and on-first-sight worker.
//! - **[`difficulty`]**. Game-difficulty CDO field tweak (drain
//!   rates, damage multipliers, regen rates, etc) with the same
//!   capture / multiplier / re-apply pattern.
//! - **[`inventory`]**. Viewport-paging hook framework
//!   ([`inventory::viewport`]) for inventory widgets that bump
//!   capacity beyond the visible grid: mouse-wheel scroll +
//!   per-widget viewport-start state + synthetic-refresh
//!   re-entrance guard + post-refresh rebind. Game crate
//!   implements a thin [`inventory::viewport::ViewportBinder`]
//!   trait with the parm shapes + bind logic.
//! - **[`damage`]**. Universal damage-event hook framework
//!   ([`damage::DamageHook`]) for the multicast / RPC every
//!   UE5 game fires per damage hit. Owns the trampoline +
//!   parm decode + `FDamageInfo` lookup + Player/Other
//!   classification + `before` / `after` dispatch. Game crate
//!   implements [`damage::DamageBinder`] to apply Critical
//!   (pre, mutate damage), Evasion (pre), Lifesteal (post),
//!   Thorns (post), kill credit (post).
//!
//! Each module wraps a low-level primitive (`StandardEffect` /
//! `FieldTweak<T>` / `ClassFieldTweak<T>` / `ProcessEventHook`)
//! with the universal apply-loop + atomic-knob + status-counter
//! pattern, so the game crate doesn't re-implement any of it.
//!
//! ## Heterogeneous adoption
//!
//! Not every UE5 mod uses every module, and not every module
//! applies cleanly to every UE5 game. ueforge supports this:
//! the modules are independent, opt-in via use sites. A pure
//! stack-size tweak only consumes [`stacks`]. An RPG-only mod
//! only consumes [`rpg`]. A mod that uses all five picks a knob
//! from each menu and ignores the rest. Game crates carry only
//! game-specific knowledge (UE class names, field offsets,
//! UFunction parm shapes); the per-game extension surface is
//! `&'static` config + an opt-in trait impl.
//!
//! The framework's design rule: each universal pattern is
//! defined ONCE in ueforge. If you find yourself writing the
//! same scaffolding in two game crates, that's a missing
//! module. File an entry under "Open: more ueforge
//! extraction candidates" in `docs/todo.md`.

pub mod args;
pub mod build;
pub mod client;
pub mod counters;
pub mod damage;
pub mod data_table;
pub mod debug;
pub mod discovery;
pub mod dynamic_tweaks;
pub mod envelope;
pub mod fall;
pub mod hook;
pub mod hot_reload;
pub mod inventory;
// log moved to modforge in Phase 0b. Both the module and the
// log!() macro re-export through this single `pub use`.
pub use modforge::log;
pub mod mod_main;
pub mod ops;
pub mod parms;
pub mod pe_queue;
pub mod rpg;
pub mod ring;
pub mod scanner;
pub mod selector;
pub mod server;
pub mod settings;
pub mod shutdown;
pub mod tweak;
pub mod uasset;
pub mod ue;
pub mod ui;
pub mod ui_class_browser;
pub mod ui_data_table_browser;
pub mod ui_dynamic_tweaks;
pub mod ui_scanner;
pub mod ui_struct_browser;
pub mod ui_tweaks;
pub mod winproc;
pub mod worker;

// Re-export modforge macros so existing call sites compile
// unchanged. Phase 0b migration: counters + ring moved into
// modforge; their macros are #[macro_export]'d there.
pub use modforge::{counter, counter_json, peak};

pub use envelope::{OpResponse, parse_request};
pub use mod_main::{ModDef, TabDef};
pub use pe_queue::{DrainStats, Queue};
pub use server::{Config, spawn};

pub fn register_standard_ops() {
    selector::register_builtins();
    ops::register_builtins();
    ops::register_with_resolver(selector::resolve);
}

pub fn start_debug_server(cfg: Config) {
    register_standard_ops();
    spawn(
        cfg,
        |body| {
            let resp = envelope::handle_request(
                body,
                &ops::OP_REGISTRY,
                || ue::try_runtime().is_some(),
            );
            serde_json::to_vec(&resp).unwrap_or_else(|_| b"{}".to_vec())
        },
        |msg| log::log(format_args!("{msg}")),
    );
}
