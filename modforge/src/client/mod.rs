//! Blocking test client for `ueforge`-embedded mods.
//!
//! Activated by the `client` feature (set in your `[dev-dependencies]`
//! line: `ueforge = { path = "...", features = ["client"] }`).
//!
//! `Api<S>` mirrors the server's `OpResponse<S>` envelope from
//! [`crate::envelope`] and provides a `ureq`-backed POST loop for
//! tests to drive ops, snapshot state, and call UFunctions.
//!
//! Run pattern (after launching the game with the mod's debug
//! endpoint enabled):
//!
//! ```text
//! set MY_MOD_DEBUG_PORT=17171
//! cargo test --test foo. --test-threads=1 --nocapture
//! ```
//!
//! Tests share a single global game state, so always pass
//! `--test-threads=1`.

use std::marker::PhantomData;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::envelope::OpResponse;

pub mod diff;
pub mod perf;
pub mod scenario;

/// Default request timeout. A test driving a slow PE-drain op
/// (e.g. one that waits for a frame to fire before the queue
/// drains) can exceed 5s; override per-Api with `with_timeout`.
pub const DEFAULT_TIMEOUT_SECS: u64 = 5;

pub struct Api<S> {
    port: u16,
    endpoint: String,
    agent: ureq::Agent,
    timeout: Duration,
    auth_token: Option<String>,
    _phantom: PhantomData<S>,
}

impl<S: DeserializeOwned> Api<S> {
    /// Connect at an explicit port. Default timeout
    /// `DEFAULT_TIMEOUT_SECS`. Chain `.with_timeout(...)` to
    /// override.
    pub fn at(port: u16, endpoint: impl Into<String>) -> Self {
        let timeout = Duration::from_secs(DEFAULT_TIMEOUT_SECS);
        let agent = ureq::AgentBuilder::new().timeout(timeout).build();
        Self {
            port,
            endpoint: endpoint.into(),
            agent,
            timeout,
            auth_token: None,
            _phantom: PhantomData,
        }
    }

    /// Override the per-request timeout.
    pub fn with_timeout(self, timeout: Duration) -> Self {
        let agent = ureq::AgentBuilder::new().timeout(timeout).build();
        Self {
            port: self.port,
            endpoint: self.endpoint,
            agent,
            timeout,
            auth_token: self.auth_token,
            _phantom: PhantomData,
        }
    }

    /// Attach a per-launch auth token. Sent as
    /// `X-Ueforge-Auth: <token>` on every request. Pair with
    /// `server::Config::auth_token` to gate the endpoint.
    pub fn with_auth(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Connect using a port from the named environment variable.
    /// Returns `None` if the env var is unset or unparseable, so
    /// tests can skip cleanly when the mod isn't running.
    pub fn try_connect(env_var: &str, endpoint: impl Into<String>) -> Option<Self> {
        let port = std::env::var(env_var).ok()?.parse::<u16>().ok()?;
        Some(Self::at(port, endpoint))
    }

    /// Like `try_connect` but panics with a clear message when the
    /// env var is missing.
    pub fn require(env_var: &str, endpoint: impl Into<String>) -> Self {
        let endpoint = endpoint.into();
        Self::try_connect(env_var, endpoint.clone()).unwrap_or_else(|| {
            panic!(
                "{env_var} not set. Launch the game with the mod's debug \
                 endpoint enabled, then export {env_var}=<port> before \
                 running tests."
            )
        })
    }

    /// POST a request, parse the response. Panics on transport or
    /// JSON errors. These are infrastructure failures, not test
    /// failures, and a panic surfaces them clearly. Use
    /// [`Self::try_op`] to handle them as `Err` instead.
    pub fn op(&self, op: &str, args: Value) -> OpResponse<S> {
        self.try_op(op, args).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Result-returning variant of [`Self::op`]. Tests that want
    /// to assert on transport failures (e.g. "the listener
    /// disappeared after a `simulate_crash` op") can use this
    /// instead of letting the helper panic.
    pub fn try_op(&self, op: &str, args: Value) -> Result<OpResponse<S>, String> {
        let body = json!({ "op": op, "args": args });
        let url = format!("http://127.0.0.1:{}{}", self.port, self.endpoint);
        let mut req = self.agent.post(&url);
        if let Some(token) = &self.auth_token {
            req = req.set("X-Ueforge-Auth", token);
        }
        let res = req
            .send_json(body)
            .map_err(|e| format!("ueforge::client POST {url} failed: {e}"))?;
        res.into_json::<OpResponse<S>>()
            .map_err(|e| format!("ueforge::client: response not valid JSON: {e}"))
    }

    /// Run an op, assert `ok=true`, return the post-op state.
    pub fn op_ok(&self, op: &str, args: Value) -> S {
        let r = self.op(op, args);
        assert!(r.ok, "op {op} failed: {:?}", r.error);
        r.state
    }

    /// `op("snapshot", null)` shortcut.
    pub fn snapshot(&self) -> S {
        self.op("snapshot", Value::Null).state
    }

    /// Snapshot without typed deserialization. Returns the raw
    /// `state` `Value`. Use when generic helpers
    /// ([`crate::client::diff`]) want to read fields by JSON
    /// path without the per-mod `Snapshot` shape getting in the
    /// way. Cheaper than `snapshot` + `serde_json::to_value`
    /// round-trip.
    pub fn snapshot_value(&self) -> Value {
        let body = json!({ "op": "snapshot", "args": Value::Null });
        let url = format!("http://127.0.0.1:{}{}", self.port, self.endpoint);
        let mut req = self.agent.post(&url);
        if let Some(token) = &self.auth_token {
            req = req.set("X-Ueforge-Auth", token);
        }
        let res = req
            .send_json(body)
            .unwrap_or_else(|e| panic!("snapshot_value POST {url} failed: {e}"));
        let envelope: Value = res
            .into_json()
            .unwrap_or_else(|e| panic!("snapshot_value: response not JSON: {e}"));
        envelope.get("state").cloned().unwrap_or(Value::Null)
    }

    /// Generic UFunction call. The endpoint exposes one `call` op;
    /// every UE-side experiment goes through it. Tests build the
    /// parm bytes from `#[repr(C)]` structs that mirror the SDK,
    /// hex-encode, send, decode the post-call buffer.
    pub fn call_ufunction(
        &self,
        class: &str,
        function: &str,
        instance_selector: &str,
        parms_bytes: &[u8],
    ) -> Result<(Vec<u8>, S), String> {
        let parms_hex = hex::encode(parms_bytes);
        let r = self.op(
            "call",
            json!({
                "class": class,
                "function": function,
                "instance_selector": instance_selector,
                "parms_hex": parms_hex,
            }),
        );
        if !r.ok {
            return Err(r.error.unwrap_or_else(|| "<no error>".into()));
        }
        let after_hex = r
            .result
            .get("parms_hex_after")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "no parms_hex_after in result".to_string())?;
        let after = hex::decode(after_hex).map_err(|e| format!("bad hex: {e}"))?;
        Ok((after, r.state))
    }

    /// Typed `call_ufunction`. Game-side parm struct is a
    /// `#[repr(C)] T` with the zerocopy derives (`FromBytes` +
    /// `IntoBytes` + `Immutable` + `KnownLayout`); this method
    /// serializes it, calls the engine, decodes the post-call
    /// buffer back into `T`, and returns it (so OUT fields the
    /// engine wrote are visible).
    ///
    /// Safe: the zerocopy trait bounds prove `T`'s layout is
    /// POD (no padding-with-pointers, no Drop, no validity
    /// invariants). No `unsafe` block needed at the call sites.
    pub fn call_ufunction_typed<T>(
        &self,
        class: &str,
        function: &str,
        instance_selector: &str,
        parms: T,
    ) -> Result<(T, S), String>
    where
        T: zerocopy::FromBytes
            + zerocopy::IntoBytes
            + zerocopy::Immutable
            + zerocopy::KnownLayout
            + Copy,
    {
        let bytes = parms.as_bytes();
        let (after, state) = self.call_ufunction(class, function, instance_selector, bytes)?;
        let out: T = T::read_from_bytes(&after).map_err(|e| {
            format!(
                "parm decode failed (bytes={} expected={}): {e}",
                after.len(),
                std::mem::size_of::<T>()
            )
        })?;
        Ok((out, state))
    }

    // ---- Standard RPG-op shortcuts ----------------------------
    //
    // Every mod that wires up `ueforge::rpg::ops` gets these for
    // free. The shortcuts call `op_ok` (panic on failure) and
    // return the post-op state. Use the raw `op` method if you
    // need to inspect failures.

    pub fn skill_spend(&self, id: &str, count: u32) -> S {
        self.op_ok("skill_spend", json!({"id": id, "count": count}))
    }

    pub fn skill_refund(&self, id: &str, count: u32) -> S {
        self.op_ok("skill_refund", json!({"id": id, "count": count}))
    }

    pub fn skill_toggle(&self, id: &str, enabled: bool) -> S {
        self.op_ok("skill_toggle", json!({"id": id, "enabled": enabled}))
    }

    pub fn set_skill_points(&self, count: u32) -> S {
        self.op_ok("set_skill_points", json!({"count": count}))
    }

    pub fn reload_slot(&self) -> S {
        self.op_ok("reload_slot", Value::Null)
    }

    /// Current `skill_levels.<id>` from the snapshot (raw value
    /// path). 0 if missing.
    pub fn skill_level(&self, id: &str) -> u32 {
        self.snapshot_value()
            .get("player_state")
            .and_then(|p| p.get("skill_levels"))
            .and_then(|m| m.get(id))
            .and_then(|n| n.as_u64())
            .map(|n| n as u32)
            .unwrap_or(0)
    }

    /// Current `skill_points` from the snapshot. 0 if missing.
    pub fn skill_points(&self) -> u32 {
        self.snapshot_value()
            .get("player_state")
            .and_then(|p| p.get("skill_points"))
            .and_then(|n| n.as_u64())
            .map(|n| n as u32)
            .unwrap_or(0)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

// UDataTable.RowMap (TMap<FName, uint8*>) at +0x30 on UE 5.x.
const ROW_MAP_OFFSET: u64 = 0x30;
// TMap layout (UE 5.x): { void* Data; i32 Num; i32 Max }.
const TMAP_HEADER_BYTES: u64 = 16;
// TSparseArray<TSetElement<TPair<K, V>>> stride = 24 bytes
const TMAP_ELEMENT_SIZE: usize = 24;

#[derive(Debug, Clone, Copy)]
pub struct DtRow {
    pub fname: u64,
    pub addr: u64,
}

impl DtRow {
    pub fn addr_selector(&self) -> String {
        format!("addr:0x{:X}", self.addr)
    }
}

#[derive(Debug, Clone)]
pub struct ClassInstance {
    pub addr_selector: String,
    pub addr: u64,
    pub name: String,
    pub full_name: String,
}

pub fn find_data_table_by_name<S: DeserializeOwned>(
    api: &Api<S>,
    short_name: &str,
) -> Option<(String, u64)> {
    let r = api.op(
        "walk_class",
        json!({"class": "DataTable", "max": 10000, "include_cdo": false}),
    );
    if !r.ok {
        return None;
    }
    let inst = r.result.get("instances")?.as_array()?.iter().find(|i| {
        i.get("name").and_then(|v| v.as_str()) == Some(short_name)
    })?;
    let sel = inst.get("addr_selector")?.as_str()?.to_string();
    let addr = inst
        .get("addr")
        .and_then(|v| v.as_str())
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())?;
    Some((sel, addr))
}

pub fn find_data_table_by_path<S: DeserializeOwned>(
    api: &Api<S>,
    path_substring: &str,
) -> Option<(String, u64)> {
    let r = api.op(
        "walk_class",
        json!({"class": "DataTable", "max": 10000, "include_cdo": false}),
    );
    if !r.ok {
        return None;
    }
    let inst = r.result.get("instances")?.as_array()?.iter().find(|i| {
        i.get("full_name")
            .and_then(|v| v.as_str())
            .map(|s| s.contains(path_substring))
            .unwrap_or(false)
    })?;
    let sel = inst.get("addr_selector")?.as_str()?.to_string();
    let addr = inst
        .get("addr")
        .and_then(|v| v.as_str())
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())?;
    Some((sel, addr))
}

pub fn read_data_table_rows<S: DeserializeOwned>(
    api: &Api<S>,
    table_addr_selector: &str,
) -> Result<Vec<DtRow>, String> {
    let header_resp = api.op(
        "read_bytes",
        json!({
            "instance_selector": table_addr_selector,
            "offset": ROW_MAP_OFFSET,
            "length": TMAP_HEADER_BYTES,
        }),
    );
    if !header_resp.ok {
        return Err(format!(
            "read_bytes header on {table_addr_selector} failed: {:?}",
            header_resp.error
        ));
    }
    let bytes = hex::decode(
        header_resp
            .result
            .get("bytes_hex")
            .and_then(|v| v.as_str())
            .ok_or("header response missing bytes_hex")?,
    )
    .map_err(|e| format!("hex decode: {e}"))?;
    if bytes.len() < 16 {
        return Err(format!("header too short: {} bytes", bytes.len()));
    }
    let data_ptr = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    let data_num = i32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if data_num <= 0 || data_ptr == 0 {
        return Ok(Vec::new());
    }

    let total_bytes = (data_num as u64) * (TMAP_ELEMENT_SIZE as u64);
    let elem_resp = api.op(
        "read_bytes",
        json!({
            "instance_selector": format!("addr:0x{data_ptr:X}"),
            "offset": 0,
            "length": total_bytes,
        }),
    );
    if !elem_resp.ok {
        return Err(format!(
            "read_bytes element-array failed: {:?}",
            elem_resp.error
        ));
    }
    let slot_bytes = hex::decode(
        elem_resp
            .result
            .get("bytes_hex")
            .and_then(|v| v.as_str())
            .ok_or("element response missing bytes_hex")?,
    )
    .map_err(|e| format!("hex decode: {e}"))?;

    let mut rows = Vec::with_capacity(data_num as usize);
    for i in 0..(data_num as usize) {
        let off = i * TMAP_ELEMENT_SIZE;
        if off + 16 > slot_bytes.len() {
            break;
        }
        let fname = u64::from_le_bytes(slot_bytes[off..off + 8].try_into().unwrap());
        let addr = u64::from_le_bytes(slot_bytes[off + 8..off + 16].try_into().unwrap());
        if addr == 0 {
            continue;
        }
        rows.push(DtRow { fname, addr });
    }
    Ok(rows)
}

pub fn fname_to_string<S: DeserializeOwned>(api: &Api<S>, fname: u64) -> Option<String> {
    let r = api.op("fname_to_string", json!({"fname": fname}));
    if !r.ok {
        return None;
    }
    r.result.get("string").and_then(|v| v.as_str()).map(String::from)
}

pub fn walk_class_instances<S: DeserializeOwned>(
    api: &Api<S>,
    class: &str,
    max: usize,
) -> Vec<ClassInstance> {
    walk_class_inner(api, class, max, false)
}

pub fn walk_class_instances_with_cdo<S: DeserializeOwned>(
    api: &Api<S>,
    class: &str,
    max: usize,
) -> Vec<ClassInstance> {
    walk_class_inner(api, class, max, true)
}

/// Walk live objects whose class chain contains `needle`.
/// Survives Blueprint reinstancing, unlike `walk_class_instances`
/// (misery research.md 22.13).
pub fn walk_class_chain_instances<S: DeserializeOwned>(
    api: &Api<S>,
    needle: &str,
    max: usize,
) -> Vec<ClassInstance> {
    let r = api.op("walk_class_chain", json!({"needle": needle, "max": max}));
    if !r.ok {
        return Vec::new();
    }
    let Some(arr) = r.result.get("instances").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter().filter_map(parse_class_instance).collect()
}

pub fn find_class_cdo<S: DeserializeOwned>(
    api: &Api<S>,
    class: &str,
) -> Option<ClassInstance> {
    let r = api.op(
        "walk_class",
        json!({"class": class, "max": 32, "include_cdo": true}),
    );
    if !r.ok {
        return None;
    }
    let arr = r.result.get("instances")?.as_array()?;
    let cdo = arr.iter().find(|i| {
        i.get("is_cdo")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    })?;
    parse_class_instance(cdo)
}

pub fn find_live_instance<S: DeserializeOwned>(
    api: &Api<S>,
    class: &str,
) -> Option<ClassInstance> {
    let r = api.op(
        "walk_class",
        json!({"class": class, "max": 32, "include_cdo": false}),
    );
    if !r.ok {
        return None;
    }
    let arr = r.result.get("instances")?.as_array()?;
    let live = arr.iter().find(|i| {
        !i.get("is_cdo")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    })?;
    parse_class_instance(live)
}

pub fn read_component_ptr<S: DeserializeOwned>(
    api: &Api<S>,
    parent_addr: u64,
    offset: u64,
) -> Option<u64> {
    let v = read_u64(api, parent_addr, offset);
    if v == 0 { None } else { Some(v) }
}

fn walk_class_inner<S: DeserializeOwned>(
    api: &Api<S>,
    class: &str,
    max: usize,
    include_cdo: bool,
) -> Vec<ClassInstance> {
    let r = api.op(
        "walk_class",
        json!({"class": class, "max": max, "include_cdo": include_cdo}),
    );
    if !r.ok {
        return Vec::new();
    }
    let Some(arr) = r.result.get("instances").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter().filter_map(parse_class_instance).collect()
}

fn parse_class_instance(inst: &serde_json::Value) -> Option<ClassInstance> {
    let sel = inst.get("addr_selector")?.as_str()?.to_string();
    let addr_str = inst.get("addr")?.as_str()?;
    let addr = u64::from_str_radix(addr_str.trim_start_matches("0x"), 16).ok()?;
    let name = inst.get("name")?.as_str()?.to_string();
    let full_name = inst
        .get("full_name")
        .and_then(|v| v.as_str())
        .unwrap_or(&name)
        .to_string();
    Some(ClassInstance {
        addr_selector: sel,
        addr,
        name,
        full_name,
    })
}

pub fn read_bytes<S: DeserializeOwned>(
    api: &Api<S>,
    addr: u64,
    offset: u64,
    length: u64,
) -> Vec<u8> {
    let r = api.op(
        "read_bytes",
        json!({
            "instance_selector": format!("addr:0x{addr:X}"),
            "offset": offset,
            "length": length,
        }),
    );
    if !r.ok {
        return Vec::new();
    }
    r.result
        .get("bytes_hex")
        .and_then(|v| v.as_str())
        .and_then(|s| hex::decode(s).ok())
        .unwrap_or_default()
}

pub fn read_i32<S: DeserializeOwned>(api: &Api<S>, addr: u64, offset: u64) -> i32 {
    let b = read_bytes(api, addr, offset, 4);
    if b.len() < 4 { return 0; }
    i32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

pub fn read_u32<S: DeserializeOwned>(api: &Api<S>, addr: u64, offset: u64) -> u32 {
    let b = read_bytes(api, addr, offset, 4);
    if b.len() < 4 { return 0; }
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

pub fn read_f32<S: DeserializeOwned>(api: &Api<S>, addr: u64, offset: u64) -> f32 {
    let b = read_bytes(api, addr, offset, 4);
    if b.len() < 4 { return 0.0; }
    f32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

pub fn read_f64<S: DeserializeOwned>(api: &Api<S>, addr: u64, offset: u64) -> f64 {
    let b = read_bytes(api, addr, offset, 8);
    if b.len() < 8 { return 0.0; }
    f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

pub fn read_u8<S: DeserializeOwned>(api: &Api<S>, addr: u64, offset: u64) -> u8 {
    let b = read_bytes(api, addr, offset, 1);
    b.first().copied().unwrap_or(0)
}

pub fn read_u64<S: DeserializeOwned>(api: &Api<S>, addr: u64, offset: u64) -> u64 {
    let b = read_bytes(api, addr, offset, 8);
    if b.len() < 8 { return 0; }
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

pub fn from_le_i32(buf: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}

pub fn from_le_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}

pub fn from_le_u64(buf: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(buf[off..off + 8].try_into().unwrap())
}

pub fn from_le_f32(buf: &[u8], off: usize) -> f32 {
    f32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}

pub fn from_le_f64(buf: &[u8], off: usize) -> f64 {
    f64::from_le_bytes(buf[off..off + 8].try_into().unwrap())
}

pub struct TArrayHeader {
    pub ptr: u64,
    pub num: i32,
    pub max: i32,
}

pub fn read_tarray_header<S: DeserializeOwned>(
    api: &Api<S>,
    addr: u64,
    offset: u64,
) -> Option<TArrayHeader> {
    let b = read_bytes(api, addr, offset, 16);
    if b.len() < 16 { return None; }
    Some(TArrayHeader {
        ptr: from_le_u64(&b, 0),
        num: from_le_i32(&b, 8),
        max: from_le_i32(&b, 12),
    })
}

pub fn fname_from_parts<S: DeserializeOwned>(
    api: &Api<S>,
    comparison_index: u32,
    number: u32,
) -> Option<String> {
    let packed: u64 = (comparison_index as u64) | ((number as u64) << 32);
    fname_to_string(api, packed)
}

#[derive(Debug, Clone)]
pub struct ModuleSamples {
    pub module: String,
    pub samples: u64,
    pub pct: f64,
}

#[derive(Debug, Clone)]
pub struct ThreadSampleRow {
    pub name: String,
    pub tid: u64,
    pub samples: u64,
    pub by_module: Vec<ModuleSamples>,
}

pub struct ThreadModulesReport {
    pub total_samples: u64,
    pub by_module_grand_total: Vec<ModuleSamples>,
    pub by_thread: Vec<ThreadSampleRow>,
}

impl ThreadModulesReport {
    pub fn from_value(v: &serde_json::Value) -> Self {
        let total_samples = v.get("total_samples").and_then(|x| x.as_u64()).unwrap_or(0);
        let by_module_grand_total = v
            .get("by_module_grand_total")
            .and_then(|x| x.as_array())
            .map(|arr| arr.iter().filter_map(parse_module).collect())
            .unwrap_or_default();
        let by_thread = v
            .get("by_thread")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        Some(ThreadSampleRow {
                            name: t.get("name").and_then(|v| v.as_str())?.to_string(),
                            tid: t.get("tid").and_then(|v| v.as_u64()).unwrap_or(0),
                            samples: t.get("samples").and_then(|v| v.as_u64()).unwrap_or(0),
                            by_module: t
                                .get("by_module")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.iter().filter_map(parse_module).collect())
                                .unwrap_or_default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            total_samples,
            by_module_grand_total,
            by_thread,
        }
    }
}

fn parse_module(v: &serde_json::Value) -> Option<ModuleSamples> {
    Some(ModuleSamples {
        module: v.get("module").and_then(|x| x.as_str())?.to_string(),
        samples: v.get("samples").and_then(|x| x.as_u64()).unwrap_or(0),
        pct: v.get("pct").and_then(|x| x.as_f64()).unwrap_or(0.0),
    })
}

impl std::fmt::Display for ThreadModulesReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\ntotal samples: {}", self.total_samples)?;
        writeln!(f, "\n=== Grand total: which module is the process IN ===")?;
        writeln!(f, "{:>40} {:>10} {:>8}", "module", "samples", "%")?;
        writeln!(f, "{}", "-".repeat(62))?;
        for m in &self.by_module_grand_total {
            writeln!(f, "{:>40} {:>10} {:>7.2}%", m.module, m.samples, m.pct)?;
        }
        writeln!(f, "\n=== Per-thread breakdown ===")?;
        for t in self.by_thread.iter().take(20) {
            writeln!(f, "\n[{}] tid={} samples={}", t.name, t.tid, t.samples)?;
            for m in &t.by_module {
                writeln!(f, "  {:>40} {:>8} {:>6.2}%", m.module, m.samples, m.pct)?;
            }
        }
        Ok(())
    }
}

pub fn sample_thread_modules<S: DeserializeOwned>(
    api: &Api<S>,
    duration_ms: u64,
    interval_ms: u64,
) -> ThreadModulesReport {
    let r = api.op(
        "sample_thread_modules",
        json!({"duration_ms": duration_ms, "interval_ms": interval_ms}),
    );
    if !r.ok {
        panic!("sample_thread_modules failed: {:?}", r.error);
    }
    ThreadModulesReport::from_value(&r.result)
}

// ---- Generic research helpers ------------------------------------
//
// These work over the JSON op protocol any modforge-based mod
// exposes. Unity and UE mods alike can use them.

/// Ping the control plane. Returns None (with a SKIP message) if
/// unreachable, so tests pass without a running game.
pub fn ping_or_skip<S: DeserializeOwned>(api: &Api<S>) -> Option<()> {
    match api.try_op("ping", json!({})) {
        Ok(r) if r.ok => Some(()),
        Ok(r) => panic!("ping not ok: {:?}", r.error),
        Err(e) => {
            eprintln!(
                "SKIP: no control plane answering ({e}); launch the game with the mod loaded"
            );
            None
        }
    }
}

/// Handle attached by the shim's serializer so ops chain.
pub fn handle_of(v: &Value) -> Option<i64> {
    v.get("handle").and_then(Value::as_i64)
}

/// Element count of a sequence: tries get_Length then get_Count.
pub fn count_of<S: DeserializeOwned>(api: &Api<S>, h: i64) -> Option<i64> {
    for getter in ["get_Length", "get_Count"] {
        let r = api.op(
            "invoke_method",
            json!({"handle": h, "method": getter, "args": []}),
        );
        if r.ok {
            return r.result.as_i64();
        }
    }
    None
}

/// Walk a sequence handle: get_Item(i) per element, inspect each,
/// print fields, release handles.
pub fn dump_sequence<S: DeserializeOwned>(api: &Api<S>, label: &str, seq: i64) {
    let Some(n) = count_of(api, seq) else {
        println!("{label}: no get_Length/get_Count answered");
        return;
    };
    println!("{label}: {n} element(s)");
    for i in 0..n {
        let item = api.op(
            "invoke_method",
            json!({"handle": seq, "method": "get_Item", "args": [i]}),
        );
        if !item.ok {
            println!("{label}[{i}]: get_Item failed: {:?}", item.error);
            continue;
        }
        let Some(eh) = handle_of(&item.result) else {
            println!("{label}[{i}] = {}", item.result);
            continue;
        };
        let inspect = api.op("inspect_object", json!({"handle": eh}));
        println!(
            "{label}[{i}]:\n{}",
            serde_json::to_string_pretty(&inspect.result).unwrap_or_default()
        );
        api.op("release_handle", json!({"handle": eh}));
    }
    api.op("release_handle", json!({"handle": seq}));
}

/// Print methods declared directly on a class (filters out inherited).
pub fn print_declared_methods<S: DeserializeOwned>(api: &Api<S>, class: &str) {
    let r = api.op("list_methods", json!({"class": class}));
    if !r.ok {
        println!("list_methods({class}) failed: {:?}", r.error);
        return;
    }
    let empty = vec![];
    let methods = r.result["methods"].as_array().unwrap_or(&empty);
    println!("{class} declares:");
    for m in methods {
        if m["declared_on"].as_str() != Some(class) {
            continue;
        }
        println!(
            "  {}({}) -> {}{}",
            m["name"].as_str().unwrap_or("?"),
            m["params"].as_i64().unwrap_or(-1),
            m["return"].as_str().unwrap_or("?"),
            if m["static"].as_bool() == Some(true) {
                " [static]"
            } else {
                ""
            },
        );
    }
}

/// Field name to value map from inspect_object.
pub fn fields<S: DeserializeOwned>(api: &Api<S>, handle: i64) -> Option<Value> {
    let r = api.op("inspect_object", json!({"handle": handle}));
    if r.ok {
        Some(r.result)
    } else {
        None
    }
}

/// Parse a Vector3 string "(x, y, z)" into a tuple.
pub fn parse_vec3(v: &Value) -> Option<(f64, f64, f64)> {
    let s = v
        .as_str()
        .or_else(|| v.get("str").and_then(Value::as_str))?;
    let s = s.trim().trim_start_matches('(').trim_end_matches(')');
    let mut parts = s.split(',').map(|p| p.trim().parse::<f64>());
    match (parts.next(), parts.next(), parts.next()) {
        (Some(Ok(x)), Some(Ok(y)), Some(Ok(z))) => Some((x, y, z)),
        _ => None,
    }
}

// ---- Reading engine shapes over the control plane ----
//
// These were each written two or three times across one game's
// research tests before being collected here. None of them is
// about a particular game: an FString is an FString in every
// Unreal title, and a UObject's class and name sit at the same
// two offsets.

/// A UE `TArray` header: `{ void* Data; int32 Num; int32 Max; }`.
pub const TARRAY_BYTES: u64 = 16;

/// A `UObject` header: its class pointer and its name.
pub const UOBJECT_CLASS: u64 = 0x10;
pub const UOBJECT_NAME: u64 = 0x18;

/// Read a `TArray` header: `(data pointer, length, capacity)`.
///
/// All zeroes when the read fails, which reads the same as an
/// empty array, and an empty array is what a caller should do
/// nothing with either way.
pub fn read_tarray<S: DeserializeOwned>(
    api: &Api<S>,
    addr: u64,
    offset: u64,
) -> (u64, usize, usize) {
    let b = read_bytes(api, addr, offset, TARRAY_BYTES);
    if b.len() < TARRAY_BYTES as usize {
        return (0, 0, 0);
    }
    let data = u64::from_le_bytes(b[0..8].try_into().unwrap());
    let num = i32::from_le_bytes(b[8..12].try_into().unwrap());
    let max = i32::from_le_bytes(b[12..16].try_into().unwrap());
    (data, num.max(0) as usize, max.max(0) as usize)
}

/// Read one pointer out of a `TArray` of pointers.
pub fn read_tarray_entry<S: DeserializeOwned>(api: &Api<S>, data: u64, index: usize) -> u64 {
    read_u64(api, data, index as u64 * 8)
}

/// A `UObject`'s own name, read out of its header and resolved
/// through the name table.
///
/// Use this when `inspect_address` will not answer, which is
/// often: it reports nothing for menu widgets and for streaming
/// levels.
///
/// NEVER call this on an address you found by reading memory
/// unless you know it is a live object. Offset 0 of a UObject is
/// its VTABLE, and asking the name table about a vtable pointer
/// took a game down three times in one evening.
pub fn object_name<S: DeserializeOwned>(api: &Api<S>, addr: u64) -> Option<String> {
    let raw = read_u64(api, addr, UOBJECT_NAME);
    if raw == 0 {
        return None;
    }
    fname_to_string(api, raw)
}

/// A `UObject`'s full name: every outer, outermost first, joined
/// with dots, the way the engine reports it.
///
/// The object's OWN name is often not the useful one. Every
/// streamed level in the game is called `PersistentLevel`; which
/// level it is lives in its outers.
///
/// Safe on a live object: each outer is itself a live object.
/// Bounded so a corrupt chain cannot loop forever.
pub fn object_full_name<S: DeserializeOwned>(api: &Api<S>, addr: u64) -> String {
    let mut parts = Vec::new();
    let mut cur = addr;
    for _ in 0..16 {
        if cur == 0 {
            break;
        }
        match object_name(api, cur) {
            Some(n) => parts.push(n),
            None => break,
        }
        cur = read_u64(api, cur, UOBJECT_OUTER);
    }
    parts.reverse();
    parts.join(".")
}

/// A `UObject`'s outer, or 0 when it has none.
pub const UOBJECT_OUTER: u64 = 0x20;

/// A `UObject`'s class pointer, or `None` when it has none.
pub fn object_class<S: DeserializeOwned>(api: &Api<S>, addr: u64) -> Option<u64> {
    let class = read_u64(api, addr, UOBJECT_CLASS);
    (class != 0).then_some(class)
}

/// Decode an `FString` parm block: `{ TCHAR* Data; int32 Num;
/// int32 Max; }`, whose characters are UTF-16 behind the pointer.
///
/// Takes the hex a `call` returned in `parms_hex_after`, so a
/// caller can read a string a game function handed back.
pub fn read_fstring<S: DeserializeOwned>(api: &Api<S>, parms_hex: &str) -> String {
    let Ok(bytes) = hex::decode(parms_hex) else {
        return String::new();
    };
    read_fstring_bytes(api, &bytes)
}

/// The same, from bytes already decoded.
pub fn read_fstring_bytes<S: DeserializeOwned>(api: &Api<S>, bytes: &[u8]) -> String {
    if bytes.len() < 16 {
        return String::new();
    }
    let ptr = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    let num = i32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if ptr == 0 || num <= 0 {
        return String::new();
    }
    let raw = read_bytes(api, ptr, 0, num as u64 * 2);
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|c| *c != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// Write bytes at an address. True when the write reported ok.
///
/// The counterpart to [`read_bytes`], and the thing three vendor
/// tests each wrote their own copy of.
pub fn write_bytes<S: DeserializeOwned>(
    api: &Api<S>,
    addr: u64,
    offset: u64,
    data: &[u8],
) -> bool {
    write_bytes_at(api, &format!("addr:0x{addr:X}"), offset, data)
}

/// The same, for a selector that is not a plain address:
/// `live_player`, `singleton:Foo`, and the rest of the grammar.
pub fn write_bytes_at<S: DeserializeOwned>(
    api: &Api<S>,
    selector: &str,
    offset: u64,
    data: &[u8],
) -> bool {
    api.op(
        "write_bytes",
        json!({
            "instance_selector": selector,
            "offset": offset,
            "bytes_hex": hex::encode(data),
        }),
    )
    .ok
}
