//! modforge: the engine-agnostic core shared by ueforge (UE5) and
//! unityforge (Unity Mono).
//!
//! Anything that does not touch UObject or Mono lives here once.
//! Both per-framework crates depend on `modforge = { path =
//! "../modforge" }` and call into it natively. There is no FFI:
//! both consumers are Rust.

pub mod args;
pub mod client;
pub mod counters;
pub mod debug;
pub mod envelope;
pub mod genome;
pub mod harness;
pub mod hook;
pub mod hot_reload;
pub mod input;
pub mod log;
pub mod ops;
pub mod patterns;
pub mod research;
pub mod ring;
pub mod rpg;
pub mod scanner;
pub mod seh;
pub mod server;
pub mod settings;
pub mod shutdown;
pub mod storyteller;
pub mod snapshots;
pub mod testkit;
pub mod ui;
pub mod vanilla;
pub mod winproc;
pub mod worker;
