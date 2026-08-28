//! Selector grammar. The workspace-standard `<Subject>Def` +
//! `<Subject>Registry` for resolving short strings to UObjects.
//!
//! ```text
//! K8s slot: Def=SelectorDef, Registry=SelectorRegistry
//!           (SELECTOR_REGISTRY singleton),
//!           Instance=&'static UObject, Controller=resolve()
//! ```
//!
//! Tests address objects in the running game by short strings:
//!
//! | form              | meaning                                    |
//! |-------------------|--------------------------------------------|
//! | `addr:0x...`      | raw object address (returned by `walk_class`) |
//! | `first_class:Foo` | first non-CDO instance of class `Foo`      |
//! | `class:Foo`       | alias for `first_class:Foo`                |
//! | `singleton:Foo`   | class default object (CDO) of class `Foo`  |
//!
//! Game-specific shorthand (`live_player`, `current_save`, ...)
//! lives in the embedding crate, registered into the same
//! [`SELECTOR_REGISTRY`] at worker init. After that,
//! [`resolve`] is the one entry point every op + scanner site
//! goes through.
//!
//! Adding a selector = one
//! `SELECTOR_REGISTRY.register(SelectorDef { ... })` line, no
//! match-statement edits.

use parking_lot::Mutex;

use crate::ue::{self, UObject};

/// One selector kind. The `resolver` returns:
/// - `None` if the input string isn't this selector's shape (the
///   registry walks to the next entry).
/// - `Some(Ok(obj))` on successful resolution.
/// - `Some(Err(msg))` if the shape matched but resolution failed
///   (e.g. `class:Foo` recognised but the class isn't loaded).
pub struct SelectorDef {
    /// Prefix or exact-match string for display / `list_selectors`.
    /// Free-form; the resolver decides what counts as a match.
    pub prefix: &'static str,
    pub summary: &'static str,
    pub resolver: fn(&str) -> Option<Result<&'static UObject, String>>,
}

/// The workspace-standard registry for selectors.
pub struct SelectorRegistry {
    entries: Mutex<Vec<SelectorDef>>,
}

impl SelectorRegistry {
    pub const fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    /// Register one selector kind. Order matters only when two
    /// kinds could match the same input (rare; resolvers are
    /// usually disjoint).
    pub fn register(&self, def: SelectorDef) {
        self.entries.lock().push(def);
    }

    pub fn register_many<I: IntoIterator<Item = SelectorDef>>(&self, defs: I) {
        let mut g = self.entries.lock();
        for d in defs {
            g.push(d);
        }
    }

    /// Walk every registered selector and return the first match.
    /// Returns `Err("unknown selector ...")` if no resolver claims
    /// the input.
    pub fn resolve(&self, selector: &str) -> Result<&'static UObject, String> {
        let g = self.entries.lock();
        for def in g.iter() {
            if let Some(r) = (def.resolver)(selector) {
                return r;
            }
        }
        Err(format!(
            "unknown selector '{selector}'; try op:'list_selectors'"
        ))
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.entries.lock().iter().map(|d| d.prefix).collect()
    }

    pub fn list_json(&self) -> serde_json::Value {
        let g = self.entries.lock();
        serde_json::json!({
            "selectors": g.iter().map(|d| serde_json::json!({
                "prefix": d.prefix,
                "summary": d.summary,
            })).collect::<Vec<_>>()
        })
    }
}

impl Default for SelectorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-wide selector registry singleton.
pub static SELECTOR_REGISTRY: SelectorRegistry = SelectorRegistry::new();

/// Resolve any selector against the global registry. Use this as
/// the canonical entry point for every op + scanner call site.
/// `fn` (not generic / closure) so it can be passed directly to
/// `ueforge::ops::register_with_resolver`.
pub fn resolve(selector: &str) -> Result<&'static UObject, String> {
    SELECTOR_REGISTRY.resolve(selector)
}

/// Register the framework's built-in selectors:
/// `addr:0x...`, `class:Foo`, `first_class:Foo`, `singleton:Foo`.
/// Call once at worker init, BEFORE any game-specific
/// `SELECTOR_REGISTRY.register(...)` calls (order doesn't affect
/// correctness for disjoint shapes; this just keeps the
/// `list_selectors` order intuitive).
pub fn register_builtins() {
    SELECTOR_REGISTRY.register_many([
        SelectorDef {
            prefix: "addr:0x...",
            summary: "Raw object address (hex)",
            resolver: try_addr,
        },
        SelectorDef {
            prefix: "first_class:<Name>",
            summary: "First non-CDO instance of <Name>",
            resolver: try_first_class,
        },
        SelectorDef {
            prefix: "class:<Name>",
            summary: "Alias for first_class:<Name>",
            resolver: try_class_alias,
        },
        SelectorDef {
            prefix: "singleton:<Name>",
            summary: "Class default object (CDO) of <Name>",
            resolver: try_singleton,
        },
    ]);
}

fn try_addr(s: &str) -> Option<Result<&'static UObject, String>> {
    let rest = s
        .strip_prefix("addr:0x")
        .or_else(|| s.strip_prefix("addr:0X"))?;
    Some(resolve_addr(s, rest))
}

fn try_first_class(s: &str) -> Option<Result<&'static UObject, String>> {
    let name = s.strip_prefix("first_class:")?;
    Some(resolve_first_class(name))
}

fn try_class_alias(s: &str) -> Option<Result<&'static UObject, String>> {
    let name = s.strip_prefix("class:")?;
    Some(resolve_first_class(name))
}

fn try_singleton(s: &str) -> Option<Result<&'static UObject, String>> {
    let name = s.strip_prefix("singleton:")?;
    Some(resolve_singleton(name))
}

fn resolve_singleton(class_name: &str) -> Result<&'static UObject, String> {
    let class =
        ue::find_class_fast(class_name).ok_or_else(|| format!("class '{class_name}' not found"))?;
    let cdo = class
        .class_default_object()
        .ok_or_else(|| format!("class '{class_name}' has no CDO"))?;
    // SAFETY: see resolve_first_class. Same lifetime-extension contract.
    let extended: &'static UObject =
        unsafe { std::mem::transmute::<&UObject, &'static UObject>(cdo) };
    Ok(extended)
}

fn resolve_addr(orig: &str, hex: &str) -> Result<&'static UObject, String> {
    let addr = u64::from_str_radix(hex, 16).map_err(|e| format!("bad address '{orig}': {e}"))?;
    if addr == 0 {
        return Err("addr is null".to_string());
    }
    let p = addr as *const UObject;
    Ok(unsafe { &*p })
}

fn resolve_first_class(class_name: &str) -> Result<&'static UObject, String> {
    let rt = ue::try_runtime().ok_or("ueforge: ue runtime not initialized")?;
    let class =
        ue::find_class_fast(class_name).ok_or_else(|| format!("class '{class_name}' not found"))?;
    let view = unsafe { ue::GObjectsView::from_image(rt.image_base, rt.platform_offsets) };
    if !view.is_valid() {
        return Err("gobjects view invalid".to_string());
    }
    for obj in view.iter() {
        if obj.is_default_object() {
            continue;
        }
        if obj.is_a(class) {
            // SAFETY: while caller holds the resulting reference, GObjects
            // is stable as long as we're on the game thread (or quiescent
            // off-thread). Lifetime extension matches the rest of the SDK.
            let extended: &'static UObject =
                unsafe { std::mem::transmute::<&UObject, &'static UObject>(obj) };
            return Ok(extended);
        }
    }
    Err(format!("no instance of '{class_name}'"))
}
