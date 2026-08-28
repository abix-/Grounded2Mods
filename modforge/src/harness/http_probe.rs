//! Poll the mod's HTTP plane until it responds.
//!
//! Wire shape (matches `modforge::envelope`):
//!   POST to `<endpoint>` (e.g. `/op`) with body
//!   `{"op": "<name>", "args": {...}}`. The server dispatches by
//!   the `op` field in the body, NOT by URL path.

use super::HttpProbe;
use std::time::{Duration, Instant};

/// Full endpoint URL (no op name appended; op goes in the body).
pub fn url_for_endpoint(probe: &HttpProbe) -> String {
    format!("http://127.0.0.1:{}{}", probe.port, probe.endpoint)
}

/// POST an op with optional JSON args, return raw response body.
/// `args` is the inner args JSON (e.g. `"{}"`); the envelope wrap
/// is built here so callers don't repeat themselves.
pub fn post(probe: &HttpProbe, op: &str, args_json: &str) -> Result<String, ureq::Error> {
    let url = url_for_endpoint(probe);
    let body = format!(r#"{{"op":"{}","args":{}}}"#, op, args_json);
    let mut req = ureq::post(&url);
    if let Some(tok) = probe.auth_token.as_deref() {
        req = req.header("X-Ueforge-Auth", tok);
    }
    let mut resp = req.content_type("application/json").send(body.as_str())?;
    Ok(resp.body_mut().read_to_string()?)
}

/// Poll `ping` until it succeeds or the deadline elapses.
pub fn wait_for_ready(probe: &HttpProbe, timeout: Duration) -> anyhow::Result<()> {
    let deadline = Instant::now() + timeout;
    let mut last_err: Option<String> = None;
    while Instant::now() < deadline {
        match post(probe, "ping", "{}") {
            Ok(_) => return Ok(()),
            Err(e) => last_err = Some(e.to_string()),
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    anyhow::bail!(
        "HTTP plane on port {} did not respond within {}s (last error: {})",
        probe.port,
        timeout.as_secs(),
        last_err.unwrap_or_default()
    )
}
