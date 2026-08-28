//! Generic debug-endpoint scaffolding for UE4SS Rust mods.
//!
//! Every mod in this workspace exposes a `POST /debug` HTTP endpoint
//! routing op-dispatch JSON. The framework supplies all the
//! universal pieces. Op routing for the standard catalog, snapshot
//! view types, op-list metadata. Game crates supply the game-
//! specific simulate_* ops + the snapshot data that requires UE
//! class layout knowledge.
//!
//! ```ignore
//! use ueforge::debug::{StandardOps, dispatch_standard};
//!
//! fn handle(body: &str) -> OpResponse {
//!     let (op, args) = parse_request(body)?;
//!     // 1. Try the standard catalog (snapshot/skill_*/reload_slot/
//!     //    set_skill_points/call/read_bytes/write_bytes/walk_class/
//!     //    class_outer_samples/sample_thread_modules).
//!     if let Some(r) = dispatch_standard(&op, &args, &TRACKER,
//!         &DISABLED, &PE_QUEUE, resolve_instance) {
//!         return to_response(&op, r);
//!     }
//!     // 2. Game-specific ops.
//!     match op.as_str() {
//!         "simulate_add_health" => to_response(&op, op_add_health(&args)),
//!         _ => error_response(&op, "unknown op"),
//!     }
//! }
//! ```

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value as Json;

use crate::args;
use crate::pe_queue::GameThread;
use crate::ring::EventRing;

// `PlayerStateView`, `CatalogEntry`, and `catalog_view` lifted
// to `modforge::debug` during Phase 0b row 19 (engine-agnostic
// half). UE-specific debug helpers (ProcessSnapshot,
// DamageRing, enqueue_pe / register_pe_call) stay below.
pub use modforge::debug::{CatalogEntry, PlayerStateView, catalog_view};

/// Recent damage / multicast PE event captured by a damage hook.
/// Universal shape: every UE5 RPG mod that observes damage wants
/// the same fields. [`DamageRing::record_hook_event`] converts the
/// hook event; the snapshot endpoint reads and serializes the ring.
#[derive(Clone, Serialize)]
pub struct DamageEvent {
    /// Wall-clock seconds since UNIX epoch. Sequences events in
    /// the same clock space as log lines.
    pub at_secs: u64,
    pub function: String,
    pub damage: f32,
    pub damage_flags: i32,
    pub type_flags: u32,
    pub current_damage_before: Option<f32>,
    pub current_damage_after: Option<f32>,
}

/// Bounded drop-oldest ring of `DamageEvent`s. Wraps `EventRing`
/// with the standard observer accessors so game crates declare
/// `static RING: DamageRing = DamageRing::new(64);` and call its
/// recording and snapshot methods directly.
pub struct DamageRing {
    inner: EventRing<DamageEvent>,
}

impl DamageRing {
    pub const fn new(capacity: usize) -> Self {
        Self {
            inner: EventRing::new(capacity),
        }
    }
    pub fn record(&self, ev: DamageEvent) {
        self.inner.record(ev);
    }
    /// Convert a decoded damage-hook event into the shared debug shape,
    /// timestamp it, and append it to this ring.
    pub fn record_hook_event(
        &self,
        function: &str,
        event: &crate::damage::DamageEvent<'_>,
        current_damage_before: Option<f32>,
        current_damage_after: Option<f32>,
    ) {
        let at_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        self.record(DamageEvent {
            at_secs,
            function: function.to_string(),
            damage: event.damage,
            damage_flags: event.damage_flags,
            type_flags: event.type_flags,
            current_damage_before,
            current_damage_after,
        });
    }
    pub fn snapshot(&self) -> Vec<DamageEvent> {
        self.inner.snapshot()
    }
    pub fn pushes(&self) -> u64 {
        self.inner.pushes()
    }
    pub fn peak(&self) -> usize {
        self.inner.peak()
    }
}

/// System-metric block that every mod's snapshot includes:
/// counters + per-process memory / CPU / threads + GObjects
/// population + per-region address-space breakdown.
///
/// Build with [`ProcessSnapshot::collect`]; embed in the game's
/// own snapshot struct via `#[serde(flatten)]` or as a named
/// field.
#[derive(Serialize)]
pub struct ProcessSnapshot {
    /// Hot-path counters JSON. Game crate supplies via
    /// `crate::counters::snapshot_json()` (or whatever the mod's
    /// counters module exposes).
    pub counters: Json,
    pub process_memory: Json,
    pub process_cpu: Json,
    pub process_threads: Json,
    pub game_population: Json,
    pub process_regions: Json,
}

impl ProcessSnapshot {
    /// Snapshot every system-metric field. `counters` is supplied
    /// by the caller (the counters module is owned by the game
    /// crate so it can name its own atomics). `top_classes` is the
    /// number of GObjects classes to surface in the population
    /// block (typical: 40).
    pub fn collect(counters: Json, top_classes: usize) -> Self {
        ProcessSnapshot {
            counters,
            process_memory: crate::winproc::process_memory_json(),
            process_cpu: crate::winproc::process_cpu_json(),
            process_threads: crate::winproc::process_threads_json(),
            game_population: crate::ue::probe::gobjects_population(top_classes),
            process_regions: crate::winproc::process_regions_json(),
        }
    }
}

/// Enqueue a closure on a game-thread `GameThread` with the
/// universal "trampoline must be firing" timeout-hint behavior.
/// `hint` is a short message appended to timeout errors so callers
/// know which trampoline owns the drain (typically the kill-hook
/// fn name).
pub fn enqueue_pe<F>(
    pe_queue: &GameThread,
    timeout: Duration,
    hint: &str,
    closure: F,
) -> Result<Json, String>
where
    F: FnOnce() -> Result<Json, String> + Send + 'static,
{
    pe_queue.queue().enqueue(closure, timeout).map_err(|e| {
        if e.contains("timed out") {
            format!("{e}. {hint}")
        } else {
            e
        }
    })
}

/// Register the `call` op with the workspace [`crate::ops::OP_REGISTRY`].
/// `call` enqueues a UFunction invocation onto the game-thread
/// `pe_queue` and blocks the HTTP listener thread until the
/// drain returns. `hint` is appended to timeout errors so users
/// know which game's drain is starved.
///
/// Game crates call this once at worker init alongside
/// [`crate::ops::register_builtins`] and
/// [`crate::ops::register_with_resolver`].
pub fn register_pe_call<R>(pe_queue: &'static GameThread, hint: &'static str, resolver: R)
where
    R: Fn(&str) -> Result<&'static crate::ue::UObject, String> + Copy + Send + Sync + 'static,
{
    crate::ops::OP_REGISTRY.register(crate::ops::OpDef::new(
        "call",
        "Call a UFunction on a resolved instance via the PE queue",
        "{class: str, function: str, instance_selector: str, parms_hex: str}",
        move |args_json| dispatch_call(args_json, pe_queue, hint, resolver),
    ));
}

fn dispatch_call<R>(
    args_json: &Json,
    pe_queue: &'static GameThread,
    hint: &'static str,
    resolver: R,
) -> Result<Json, String>
where
    R: Fn(&str) -> Result<&'static crate::ue::UObject, String> + Copy + Send + 'static,
{
    let class = args::arg_str(args_json, "class")?.to_string();
    let function = args::arg_str(args_json, "function")?.to_string();
    let selector = args::arg_str(args_json, "instance_selector")?.to_string();
    let parms =
        hex::decode(args::arg_str(args_json, "parms_hex")?).map_err(|e| format!("bad hex: {e}"))?;
    enqueue_pe(pe_queue, Duration::from_secs(5), hint, move || {
        let instance = resolver(&selector)?;
        let mut out = crate::ops::exec_call(instance, &class, &function, parms)?;
        if let Some(obj) = out.as_object_mut() {
            obj.insert("selector".into(), Json::String(selector.clone()));
        }
        Ok(out)
    })
}
