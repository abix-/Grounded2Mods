//! modforge: the engine-agnostic core shared by ueforge (UE5) and
//! unityforge (Unity Mono).
//!
//! Anything that does not touch UObject or Mono lives here once.
//! Both per-framework crates depend on `modforge = { path =
//! "../modforge" }` and call into it natively. There is no FFI:
//! both consumers are Rust.

pub mod actions;
pub mod actor;
pub mod annex;
pub mod args;
pub mod biome;
pub mod brain;
pub mod client;
pub mod combat;
pub mod counters;
pub mod crafting;
pub mod debug;
pub mod envelope;
pub mod faction;
pub mod genome;
pub mod harness;
pub mod hook;
pub mod hud;
pub mod hot_reload;
pub mod input;
pub mod item;
pub mod log;
pub mod memory;
pub mod mission;
pub mod monument;
pub mod ops;
pub mod patterns;
pub mod quality;
pub mod read_once;
pub mod research;
pub mod ring;
pub mod roll;
pub mod route;
pub mod rpg;
pub mod scanner;
pub mod seh;
pub mod server;
pub mod settings;
pub mod shutdown;
pub mod storyteller;
pub mod structure;
pub mod studs;
pub mod survival;
pub mod snapshots;
pub mod testkit;
pub mod ui;
pub mod unknown;
pub mod upgrade;
pub mod vanilla;
pub mod vendor;
pub mod winproc;
pub mod worker;
pub mod worldgen;
