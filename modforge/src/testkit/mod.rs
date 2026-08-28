//! Cross-game research + assertion test toolkit.
//!
//! Authoritative on: generic test primitives shared across every
//! native-PE mod target (horsey-mod, grounded2-mod, schedule1,
//! outworld-station-mod, future games). Each primitive is dual-purpose:
//! it does research (drives the running game, observes state) AND
//! asserts (fails on regression).
//!
//! The pattern was hardened in horsey-mod (see
//! `horsey-mod/tests/README.md`). The extraction plan lives in
//! `docs/todo.md` under "Extract cross-game research testkit to
//! modforge."
//!
//! Each module exports:
//! - A config struct, env-parseable via `from_env(prefix)`.
//! - A `run(&RunningGame, &Config) -> Result<...>` entry point.
//! - Pure helpers (matchers, parsers) that don't need a game.
//!
//! Game-specific resolution (e.g. "what's the address of `horse_ptr`?")
//! is passed in by the caller as a closure. The testkit itself never
//! hardcodes a game's data layout.

pub mod arm_lifecycle;
pub mod assets;
pub mod build_info;
pub mod fn_entry;
pub mod json;
pub mod msvc;
pub mod op;
pub mod recipes;
pub mod registry;
pub mod snapshot;
pub mod watch;
pub mod xrefs;
