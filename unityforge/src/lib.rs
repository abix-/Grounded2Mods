//! unityforge: Unity Mono framework. Peer of ueforge.
//!
//! Game mods compile as Rust cdylibs and depend on this crate.
//! A small C# shim project (cs-shim/) bootstraps the Rust DLL
//! inside BepInEx and exposes a function-pointer bridge for
//! Mono reflection + HarmonyLib patching. Calls through that
//! bridge are wrapped in idiomatic Rust types here.
//!
//! Engine-agnostic infrastructure (op envelope, registry, server,
//! settings, etc.) lives in modforge and is consumed natively.

pub mod bridge;
pub mod client;
pub mod ffi;
pub mod hook;
pub mod il2cpp;
pub mod input;
pub mod main_thread_queue;
pub mod mod_main;
pub mod mono;
pub mod ops;
pub mod rpg;
pub mod selector;
pub mod unity;

pub use mod_main::ModDef;
