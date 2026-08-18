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

use serde::Deserialize;
use serde_json::Value;

const ENV_PORT: &str = "MISERY_DEBUG_PORT";

/// Mirrors `misery_mod::debug::Snapshot`.
#[derive(Debug, Deserialize, Default)]
pub struct Snapshot {
    #[serde(default)]
    pub offsets_known: bool,
}

pub type Api = ueforge::client::Api<Snapshot>;

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
    api.snapshot().offsets_known
}

/// Pretty-print whatever an op returned, ok or not.
pub fn show(label: &str, r: &ueforge::OpResponse<Snapshot>) {
    if r.ok {
        println!("{label}: {}", serde_json::to_string(&r.result).unwrap_or_default());
    } else {
        println!("{label}: FAILED {:?}", r.error);
    }
}

/// First non-CDO instance of a class, via walk_class.
///
/// walk_class answers `{class, instances: [...], returned, total}`,
/// not a bare array. Reading it as an array yields "0 instances"
/// for a class that is right there.
pub fn first_instance(api: &Api, class: &str) -> Option<Value> {
    let r = api.op("walk_class", serde_json::json!({"class": class}));
    if !r.ok {
        println!("walk_class({class}) failed: {:?}", r.error);
        return None;
    }
    let arr = r.result["instances"].as_array().cloned().unwrap_or_default();
    println!("walk_class({class}): {} instance(s)", arr.len());
    arr.into_iter().find(|i| i["is_cdo"].as_bool() != Some(true))
}

/// The selector string for an instance returned by walk_class.
pub fn selector_of(inst: &Value) -> Option<String> {
    inst["addr_selector"].as_str().map(str::to_string)
}

/// read_bytes at a selector + offset, decoded little-endian.
/// The op names its argument `instance_selector`, not `selector`.
pub fn read_bytes(api: &Api, selector: &str, offset: u64, length: u64) -> Option<Vec<u8>> {
    let r = api.op(
        "read_bytes",
        serde_json::json!({"instance_selector": selector, "offset": offset, "length": length}),
    );
    if !r.ok {
        println!("read_bytes(+0x{offset:x}) failed: {:?}", r.error);
        return None;
    }
    let Some(hex_str) = r
        .result
        .as_str()
        .or_else(|| r.result["bytes_hex"].as_str())
        .or_else(|| r.result["hex"].as_str())
    else {
        // Silently returning None here hid a working read behind
        // "no output at all". Show what came back instead.
        println!("read_bytes(+0x{offset:x}): unexpected shape {}", r.result);
        return None;
    };
    match hex::decode(hex_str) {
        Ok(b) => Some(b),
        Err(e) => {
            println!("read_bytes: undecodable {hex_str:?} ({e})");
            None
        }
    }
}

pub fn as_f64(bytes: &[u8]) -> Option<f64> {
    Some(f64::from_le_bytes(bytes.get(..8)?.try_into().ok()?))
}

pub fn as_i32(bytes: &[u8]) -> Option<i32> {
    Some(i32::from_le_bytes(bytes.get(..4)?.try_into().ok()?))
}
