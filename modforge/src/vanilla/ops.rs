//! HTTP cmdlets for vanilla function invocation. Two ops register
//! against a consumer mod's OpRegistry; they bind dynamically to
//! the consumer's `'static Resolver`.
//!
//! - `vanilla.invoke`: call any function in the registry by name.
//!   Args + return are typed per the registered Signature.
//! - `vanilla.list`: enumerate every callable target (those with
//!   `TargetKind::FunctionEntry { signature: Some(_) }`).
//!
//! Wire-up: in your consumer's worker init, after building the
//! `&'static Resolver`, call `vanilla::ops::register(&OP_REGISTRY, &MY_RESOLVER)`.

use crate::ops::{OpDef, OpRegistry};
use crate::patterns::sleuth::{Resolver, TargetKind};
use crate::vanilla::invoker::Invoker;
use crate::vanilla::sig::{ArgKind, ArgValue, RetKind, RetValue, Signature};
use serde_json::{Value, json};

/// Register `vanilla.invoke` and `vanilla.list` ops against
/// `op_registry`, bound to `resolver`.
///
/// `resolver` MUST be `&'static` (i.e. a singleton bound to a
/// `&'static TargetRegistry`). The handlers capture it by
/// reference and run on every HTTP request.
pub fn register(op_registry: &OpRegistry, resolver: &'static Resolver) {
    op_registry.register(OpDef::new(
        "vanilla.invoke",
        "Invoke a function registered in the target registry. \
         Body: {target: string, args: [{kind, value}, ...], safe?: bool}. \
         Returns: {ret: {kind, value}, ok: bool, elapsed_us: u64} or {ok:false, error:string}.",
        "{target: string, args: array, safe?: bool}",
        move |args: &Value| invoke_handler(resolver, args),
    ));

    op_registry.register(OpDef::new(
        "vanilla.list",
        "List every callable target in the registry: name, address, signature.",
        "",
        move |_args: &Value| list_handler(resolver),
    ));
}

fn invoke_handler(resolver: &Resolver, args: &Value) -> Result<Value, String> {
    let target = args
        .get("target")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing or non-string 'target'".to_string())?;
    let arg_json = args
        .get("args")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "missing or non-array 'args'".to_string())?;
    let safe = args.get("safe").and_then(Value::as_bool).unwrap_or(true);

    let parsed_args: Vec<ArgValue> = arg_json
        .iter()
        .enumerate()
        .map(|(i, a)| parse_arg(a).map_err(|e| format!("args[{i}]: {e}")))
        .collect::<Result<_, _>>()?;

    let invoker = Invoker::new(resolver);
    let t0 = std::time::Instant::now();
    let result = if safe {
        invoker.call(target, &parsed_args)
    } else {
        // SAFETY: caller explicitly opted in via `safe: false`.
        unsafe { invoker.call_unsafe(target, &parsed_args) }
    };
    let elapsed_us = t0.elapsed().as_micros() as u64;

    match result {
        Ok(rv) => Ok(json!({
            "ok": true,
            "ret": ret_to_json(&rv),
            "elapsed_us": elapsed_us,
        })),
        Err(e) => Ok(json!({
            "ok": false,
            "error": format!("{e}"),
            "elapsed_us": elapsed_us,
        })),
    }
}

fn list_handler(resolver: &Resolver) -> Result<Value, String> {
    let resolved = resolver.resolve_all();
    let mut entries: Vec<Value> = resolver
        .registry()
        .iter()
        .filter_map(|def| {
            if let TargetKind::FunctionEntry {
                signature: Some(sig),
            } = def.kind
            {
                let r = resolved.get(def.name);
                Some(json!({
                    "name": def.name,
                    "addr": r.and_then(|x| x.value).map(|v| format!("0x{v:x}")),
                    "signature": signature_to_json(sig),
                    "from_hint": r.map(|x| x.from_hint).unwrap_or(false),
                }))
            } else {
                None
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
    });
    Ok(json!({ "targets": entries }))
}

fn parse_arg(v: &Value) -> Result<ArgValue, String> {
    let kind_s = v
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing 'kind'".to_string())?;
    let val = v
        .get("value")
        .ok_or_else(|| "missing 'value'".to_string())?;
    Ok(match kind_s {
        "i8" => ArgValue::I8(val.as_i64().ok_or("value not int")? as i8),
        "i16" => ArgValue::I16(val.as_i64().ok_or("value not int")? as i16),
        "i32" => ArgValue::I32(val.as_i64().ok_or("value not int")? as i32),
        "i64" => ArgValue::I64(val.as_i64().ok_or("value not int")?),
        "u8" => ArgValue::U8(val.as_u64().ok_or("value not uint")? as u8),
        "u16" => ArgValue::U16(val.as_u64().ok_or("value not uint")? as u16),
        "u32" => ArgValue::U32(val.as_u64().ok_or("value not uint")? as u32),
        "u64" => parse_u64_arg(val).map(ArgValue::U64)?,
        "ptr" => parse_u64_arg(val).map(ArgValue::Ptr)?,
        "bool" => ArgValue::Bool(val.as_bool().ok_or("value not bool")?),
        "f32" => ArgValue::F32(val.as_f64().ok_or("value not f64")? as f32),
        "f64" => ArgValue::F64(val.as_f64().ok_or("value not f64")?),
        other => return Err(format!("unknown kind: {other}")),
    })
}

fn parse_u64_arg(v: &Value) -> Result<u64, String> {
    if let Some(u) = v.as_u64() {
        return Ok(u);
    }
    if let Some(s) = v.as_str() {
        let h = s.trim_start_matches("0x").trim_start_matches("0X");
        return u64::from_str_radix(h, 16).map_err(|e| format!("hex u64 parse '{s}': {e}"));
    }
    Err("u64 value must be number or hex string".into())
}

fn ret_to_json(rv: &RetValue) -> Value {
    match *rv {
        RetValue::Void => json!({ "kind": "void" }),
        RetValue::I8(v) => json!({ "kind": "i8",  "value": v }),
        RetValue::I16(v) => json!({ "kind": "i16", "value": v }),
        RetValue::I32(v) => json!({ "kind": "i32", "value": v }),
        RetValue::I64(v) => json!({ "kind": "i64", "value": v }),
        RetValue::U8(v) => json!({ "kind": "u8",  "value": v }),
        RetValue::U16(v) => json!({ "kind": "u16", "value": v }),
        RetValue::U32(v) => json!({ "kind": "u32", "value": v }),
        RetValue::U64(v) => json!({ "kind": "u64", "value": format!("0x{v:x}") }),
        RetValue::Ptr(v) => json!({ "kind": "ptr", "value": format!("0x{v:x}") }),
        RetValue::Bool(v) => json!({ "kind": "bool", "value": v }),
        RetValue::F32(v) => json!({ "kind": "f32", "value": v }),
        RetValue::F64(v) => json!({ "kind": "f64", "value": v }),
    }
}

fn signature_to_json(sig: &Signature) -> Value {
    json!({
        "args": sig.args.iter().map(|a| arg_kind_name(*a)).collect::<Vec<_>>(),
        "ret": ret_kind_name(sig.ret),
    })
}

fn arg_kind_name(k: ArgKind) -> &'static str {
    match k {
        ArgKind::I8 => "i8",
        ArgKind::I16 => "i16",
        ArgKind::I32 => "i32",
        ArgKind::I64 => "i64",
        ArgKind::U8 => "u8",
        ArgKind::U16 => "u16",
        ArgKind::U32 => "u32",
        ArgKind::U64 => "u64",
        ArgKind::Ptr => "ptr",
        ArgKind::Bool => "bool",
        ArgKind::F32 => "f32",
        ArgKind::F64 => "f64",
    }
}

fn ret_kind_name(k: RetKind) -> &'static str {
    match k {
        RetKind::Void => "void",
        RetKind::I8 => "i8",
        RetKind::I16 => "i16",
        RetKind::I32 => "i32",
        RetKind::I64 => "i64",
        RetKind::U8 => "u8",
        RetKind::U16 => "u16",
        RetKind::U32 => "u32",
        RetKind::U64 => "u64",
        RetKind::Ptr => "ptr",
        RetKind::Bool => "bool",
        RetKind::F32 => "f32",
        RetKind::F64 => "f64",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_arg_handles_each_kind() {
        assert!(matches!(
            parse_arg(&json!({"kind":"i32","value":42})).unwrap(),
            ArgValue::I32(42)
        ));
        assert!(matches!(
            parse_arg(&json!({"kind":"u32","value":42})).unwrap(),
            ArgValue::U32(42)
        ));
        assert!(matches!(
            parse_arg(&json!({"kind":"bool","value":true})).unwrap(),
            ArgValue::Bool(true)
        ));
        assert!(matches!(
            parse_arg(&json!({"kind":"f64","value":3.14})).unwrap(),
            ArgValue::F64(_)
        ));
    }

    #[test]
    fn parse_arg_accepts_hex_string_for_u64_and_ptr() {
        let v = parse_arg(&json!({"kind":"u64","value":"0xdeadbeef"})).unwrap();
        assert!(matches!(v, ArgValue::U64(0xdeadbeef)));
        let v = parse_arg(&json!({"kind":"ptr","value":"0x140000000"})).unwrap();
        assert!(matches!(v, ArgValue::Ptr(0x140000000)));
    }

    #[test]
    fn parse_arg_rejects_unknown_kind() {
        let r = parse_arg(&json!({"kind":"nope","value":0}));
        assert!(r.is_err());
    }

    #[test]
    fn ret_to_json_hex_encodes_u64_and_ptr() {
        let v = ret_to_json(&RetValue::U64(0xdeadbeef));
        assert_eq!(v["kind"], "u64");
        assert_eq!(v["value"], "0xdeadbeef");
        let v = ret_to_json(&RetValue::Ptr(0x140000000));
        assert_eq!(v["kind"], "ptr");
        assert_eq!(v["value"], "0x140000000");
        let v = ret_to_json(&RetValue::Void);
        assert_eq!(v["kind"], "void");
    }

    #[test]
    fn signature_to_json_round_trips_kinds() {
        const SIG: Signature =
            Signature::new(&[ArgKind::Ptr, ArgKind::I32, ArgKind::F64], RetKind::U32);
        let v = signature_to_json(&SIG);
        assert_eq!(v["args"][0], "ptr");
        assert_eq!(v["args"][1], "i32");
        assert_eq!(v["args"][2], "f64");
        assert_eq!(v["ret"], "u32");
    }
}
