//! Shared helpers for the bossgangsters-mod research tests.
//!
//! The Boss Gangsters Nightlife is Unity Mono, so class names
//! are plain and every op chains on the shim's object handles.
//! New research tests reuse these; do not copy them into a
//! test file.
//!
//! Each --test target compiles this module separately and no
//! target uses every helper, so dead_code is expected here.
#![allow(dead_code, unused_imports)]

pub use unityforge::client::ping_or_skip;

use serde_json::Value;
use unityforge::client::Api;

/// Control plane port. Override with BOSSGANGSTERS_MOD_PORT.
pub fn api() -> Api<Value> {
    let port = std::env::var("BOSSGANGSTERS_MOD_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(17176);
    Api::at(port, "/op").with_timeout(std::time::Duration::from_secs(30))
}
