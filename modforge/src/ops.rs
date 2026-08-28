//! Generic op registry. Engine-agnostic Def + Registry + dispatch +
//! list_ops + per-op metrics. Per-engine handlers (UE-specific
//! ops, Unity-specific ops) register against this same registry
//! from their own crate.
//!
//! K8s slot: Def=OpDef, Registry=OpRegistry (OP_REGISTRY singleton),
//!           Instance=per-call (args, result), Controller=OpRegistry::dispatch

use parking_lot::Mutex;
use serde_json::Value as Json;

pub type OpHandler = std::sync::Arc<dyn Fn(&Json) -> Result<Json, String> + Send + Sync>;

/// One debug-op declaration. The handler is a closure (boxed to
/// erase per-game capture types. Tracker references, selector
/// resolvers, main-thread queue handles).
pub struct OpDef {
    pub name: &'static str,
    pub summary: &'static str,
    /// Human-readable args spec for the auto-generated `list_ops`
    /// response. Free-form: `"{slot: str, count?: u32}"`. Empty
    /// string for ops with no args.
    pub args: &'static str,
    pub handler: OpHandler,
}

impl OpDef {
    /// Convenience constructor.
    pub fn new<F>(name: &'static str, summary: &'static str, args: &'static str, handler: F) -> Self
    where
        F: Fn(&Json) -> Result<Json, String> + Send + Sync + 'static,
    {
        Self {
            name,
            summary,
            args,
            handler: std::sync::Arc::new(handler),
        }
    }
}

/// The workspace-standard `<Subject>Registry` for debug ops.
/// Populated at init time via [`Self::register`]; dispatch is one
/// linear scan (registries hold ~20 ops, lookups are once per
/// HTTP request, so the scan cost is invisible).
pub struct OpRegistry {
    entries: Mutex<Vec<OpDef>>,
}

impl OpRegistry {
    pub const fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    /// Register one op. Callers (the framework's `register_builtins`
    /// + each game crate's worker init) push their declarations
    /// once at startup.
    pub fn register(&self, def: OpDef) {
        self.entries.lock().push(def);
    }

    /// Register a batch of ops in one shot.
    pub fn register_many<I: IntoIterator<Item = OpDef>>(&self, defs: I) {
        let mut g = self.entries.lock();
        for d in defs {
            g.push(d);
        }
    }

    /// Look up and invoke. Returns `None` if the op isn't
    /// registered (caller falls through to "unknown op" error).
    ///
    /// The handler runs OUTSIDE the registry mutex. The lock only
    /// protects the lookup. This matters for two reasons:
    /// (1) slow handlers (discovery refresh, GObjects walks) do
    /// not serialize the entire HTTP surface; (2) a handler that
    /// crashes the worker via SEH would otherwise poison the
    /// parking_lot mutex forever, hanging every subsequent request.
    pub fn dispatch(&self, name: &str, args: &Json) -> Option<Result<Json, String>> {
        let handler = {
            let g = self.entries.lock();
            g.iter().find(|o| o.name == name)?.handler.clone()
        };
        let started = std::time::Instant::now();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(args)));
        let elapsed_ns = started.elapsed().as_nanos() as u64;
        record_op_latency(name, elapsed_ns, matches!(result, Ok(Ok(_))));
        Some(match result {
            Ok(r) => r,
            Err(_) => Err(format!("handler '{name}' panicked")),
        })
    }

    /// Names of every registered op. For "supported ops: [...]"
    /// error messages.
    pub fn names(&self) -> Vec<&'static str> {
        self.entries.lock().iter().map(|o| o.name).collect()
    }

    /// Count of registered ops.
    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }

    /// Auto-generated `list_ops` payload: `{ ops: [{ name, summary,
    /// args }, ...] }`. The framework registers a `list_ops` op
    /// that returns this; clients use it for discovery.
    pub fn list_json(&self) -> Json {
        let g = self.entries.lock();
        serde_json::json!({
            "ops": g.iter().map(|o| serde_json::json!({
                "name": o.name,
                "summary": o.summary,
                "args": o.args,
            })).collect::<Vec<_>>()
        })
    }
}

impl Default for OpRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-wide debug-op registry singleton. Game crates push
/// their ops here at worker init; the HTTP `handle()` calls
/// `OP_REGISTRY.dispatch(op, args)` for every request.
pub static OP_REGISTRY: OpRegistry = OpRegistry::new();

// ---- Per-op latency metrics --------------------------------------------

#[derive(Default, Debug, Clone)]
struct OpMetrics {
    calls: u64,
    errors: u64,
    total_ns: u64,
    max_ns: u64,
}

static METRICS: std::sync::OnceLock<Mutex<std::collections::HashMap<String, OpMetrics>>> =
    std::sync::OnceLock::new();

fn metrics_map() -> &'static Mutex<std::collections::HashMap<String, OpMetrics>> {
    METRICS.get_or_init(|| Mutex::new(std::collections::HashMap::with_capacity(64)))
}

fn record_op_latency(name: &str, elapsed_ns: u64, ok: bool) {
    let mut g = metrics_map().lock();
    let e = g.entry(name.to_string()).or_default();
    e.calls = e.calls.saturating_add(1);
    if !ok {
        e.errors = e.errors.saturating_add(1);
    }
    e.total_ns = e.total_ns.saturating_add(elapsed_ns);
    if elapsed_ns > e.max_ns {
        e.max_ns = elapsed_ns;
    }
}

/// JSON snapshot of every recorded op's metrics. Sorted by
/// total_ns descending so the slow ones surface first.
pub fn metrics_json() -> Json {
    let g = metrics_map().lock();
    let mut items: Vec<(String, OpMetrics)> =
        g.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    items.sort_by(|a, b| b.1.total_ns.cmp(&a.1.total_ns));
    let entries: Vec<Json> = items
        .into_iter()
        .map(|(name, m)| {
            let avg_ns = if m.calls > 0 { m.total_ns / m.calls } else { 0 };
            serde_json::json!({
                "name": name,
                "calls": m.calls,
                "errors": m.errors,
                "total_ns": m.total_ns,
                "max_ns": m.max_ns,
                "avg_ns": avg_ns,
            })
        })
        .collect();
    serde_json::json!({ "ops": entries })
}
