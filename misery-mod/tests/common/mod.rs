//! Shared client for the misery-mod research tests.
//!
//! Tests need MISERY running with the mod loaded. Set
//! `MISERY_DEBUG_PORT` to enable; if unset the tests skip with a
//! clear message rather than failing. The mod listens on 17176
//! (`misery-mod/src/lib.rs::DEBUG_PORT`).
//!
//! ```text
//! MISERY_DEBUG_PORT=17176 cargo test -p misery-mod --test research_emission -- --test-threads=1 --nocapture
//! ```
//!
//! Every test shares one live game, so always pass
//! `--test-threads=1`.

#![allow(dead_code)]

const ENV_PORT: &str = "MISERY_DEBUG_PORT";

/// The server's `state` field is a bool: true when the UE
/// runtime is initialised (`ueforge::start_debug_server`).
pub type Api = ueforge::client::Api<bool>;

/// Connect, or print a SKIP line and return None.
pub fn api_or_skip() -> Option<Api> {
    let Some(api) = Api::try_connect(ENV_PORT, "/debug") else {
        eprintln!("SKIP: set {ENV_PORT}=17176 and launch MISERY with misery-mod loaded");
        return None;
    };
    match api.try_op("ping", serde_json::json!({})) {
        Ok(_) => Some(api),
        Err(e) => {
            eprintln!("SKIP: control plane not answering ({e})");
            None
        }
    }
}

/// True when the UE runtime is initialised, meaning the platform
/// offsets in `lib.rs::STEAM` are non-zero AND took effect. Every
/// object-walking op fails without it, and the failure looks like
/// "no instances" rather than "not wired up", so check this first.
pub fn offsets_live(api: &Api) -> bool {
    api.snapshot()
}

/// Pretty-print whatever an op returned, ok or not.
pub fn show(label: &str, r: &ueforge::OpResponse<bool>) {
    if r.ok {
        println!(
            "{label}: {}",
            serde_json::to_string(&r.result).unwrap_or_default()
        );
    } else {
        println!("{label}: FAILED {:?}", r.error);
    }
}
