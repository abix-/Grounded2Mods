//! Op request / response envelope.
//!
//! Wire shape:
//! ```json
//! request:  { "op": "<name>", "args": {...} }
//! response: { "ok": bool, "op": "<echoed>", "error": null|string,
//!             "result": <op-specific>, "state": <snapshot> }
//! ```
//!
//! `OpResponse` is generic over `S: Serialize` so each game crate
//! supplies its own snapshot type. The envelope stays uniform; the
//! payload is whatever that game wants to expose.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

#[derive(Serialize, Deserialize)]
pub struct OpResponse<S> {
    pub ok: bool,
    pub op: String,
    pub error: Option<String>,
    #[serde(default = "default_result")]
    pub result: Json,
    pub state: S,
}

fn default_result() -> Json {
    Json::Null
}

impl<S> OpResponse<S> {
    pub fn ok(op: &str, result: Json, state: S) -> Self {
        Self {
            ok: true,
            op: op.to_string(),
            error: None,
            result,
            state,
        }
    }

    pub fn err(op: &str, msg: impl Into<String>, state: S) -> Self {
        Self {
            ok: false,
            op: op.to_string(),
            error: Some(msg.into()),
            result: Json::Null,
            state,
        }
    }

    pub fn from_result(op: &str, r: Result<Json, String>, state: S) -> Self {
        match r {
            Ok(v) => Self::ok(op, v, state),
            Err(e) => Self::err(op, e, state),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Insta snapshots: lock the wire shape so envelope refactors
    // surface as reviewable diffs instead of silent breakage.
    // Run `cargo insta review` to accept/reject after changes.

    #[test]
    fn snapshot_ok_response() {
        let r: OpResponse<serde_json::Value> =
            OpResponse::ok("snapshot", json!({"foo": 42}), json!({"alive": true}));
        insta::assert_json_snapshot!(r);
    }

    #[test]
    fn snapshot_err_response() {
        let r: OpResponse<serde_json::Value> =
            OpResponse::err("snapshot", "no slot active", json!({"alive": false}));
        insta::assert_json_snapshot!(r);
    }

    #[test]
    fn snapshot_from_result_ok() {
        let r: OpResponse<serde_json::Value> = OpResponse::from_result(
            "spend_skill",
            Ok(json!({"spent": 1, "level": 4})),
            json!({"xp": 1234}),
        );
        insta::assert_json_snapshot!(r);
    }

    #[test]
    fn snapshot_from_result_err() {
        let r: OpResponse<serde_json::Value> = OpResponse::from_result(
            "spend_skill",
            Err("insufficient skill points".to_string()),
            json!({"xp": 1234}),
        );
        insta::assert_json_snapshot!(r);
    }
}

/// Parse a request body into `(op, args)`. `args` defaults to `Null`
/// if missing. The op string is empty when the field is absent so
/// callers can produce a uniform "missing op" error.
pub fn parse_request(body: &str) -> Result<(String, Json), String> {
    let v: Json = serde_json::from_str(body).map_err(|e| format!("body json: {e}"))?;
    let op = v.get("op").and_then(Json::as_str).unwrap_or("").to_string();
    let args = v.get("args").cloned().unwrap_or(Json::Null);
    Ok((op, args))
}

/// Standard request handler shared across all mod debug endpoints.
/// Parses the body, dispatches "snapshot" and empty-op cases, then
/// falls through to the op registry. The `snapshot` closure builds
/// the game-specific snapshot for the response envelope.
pub fn handle_request<S: Serialize>(
    body: &str,
    registry: &crate::ops::OpRegistry,
    snapshot: impl Fn() -> S,
) -> OpResponse<S> {
    let (op, args) = match parse_request(body) {
        Ok(v) => v,
        Err(e) => return OpResponse::err("<parse-error>", e, snapshot()),
    };
    if op == "snapshot" {
        return OpResponse::ok(&op, Json::Null, snapshot());
    }
    if op.is_empty() {
        return OpResponse::err(
            "<missing>",
            "missing 'op' field; try op:'list_ops' for the catalog",
            snapshot(),
        );
    }
    match registry.dispatch(&op, &args) {
        Some(r) => OpResponse::from_result(&op, r, snapshot()),
        None => OpResponse::err(
            &op,
            format!("unknown op '{op}'; try op:'list_ops' for the catalog"),
            snapshot(),
        ),
    }
}
