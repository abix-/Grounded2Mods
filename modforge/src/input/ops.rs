//! HTTP cmdlets that expose [`super::l1`] and [`super::l2`] over the
//! modforge op registry.
//!
//! First-slice surface (drag/scroll/combo/replay/state.get deferred):
//!
//! - `input.mouse.move {x, y, backend?, hwnd?}`
//! - `input.mouse.click {button, x, y, backend?, hwnd?, mods?}`
//! - `input.key.down {key, backend?, hwnd?}`
//! - `input.key.up   {key, backend?, hwnd?}`
//! - `input.key.press {key, hold_ms?, backend?, hwnd?}`
//! - `input.cursor.get` -> screen-pixel cursor pos via `GetCursorPos`
//!
//! `backend` defaults to `"l2"` when an `hwnd` arg is supplied OR a
//! foreground window is available; falls back to `"l1"` otherwise.
//!
//! `hwnd` is hex string (e.g. `"0x1234abcd"`). When omitted, L2 uses
//! `GetForegroundWindow`. L2 coords are CLIENT-RELATIVE to that hwnd;
//! L1 coords are SCREEN pixels.

use serde_json::{Value as Json, json};

use crate::ops::OpDef;

use super::{Axis, Backend, Button, InputSurface, Key, PlayerCommand, l1, l2};

/// Run an action through the registered L3 [`InputSurface`] if any;
/// otherwise log a one-line warning and run `fallback` (typically L1).
fn l3_or_fallback<L3, FB>(action_name: &str, l3_action: L3, fallback: FB) -> Result<(), String>
where
    L3: FnOnce(&'static dyn InputSurface) -> Result<(), String>,
    FB: FnOnce() -> Result<(), String>,
{
    match super::input_surface() {
        Some(surface) => l3_action(surface),
        None => {
            crate::log!(
                "[input] L3 '{action_name}' requested but no surface registered; falling back to L1"
            );
            fallback()
        }
    }
}

fn arg_i32(args: &Json, name: &str) -> Result<i32, String> {
    args.get(name)
        .and_then(Json::as_i64)
        .ok_or_else(|| format!("missing arg '{name}' (i32)"))
        .and_then(|v| i32::try_from(v).map_err(|_| format!("arg '{name}' = {v} out of i32 range")))
}

fn arg_u32(args: &Json, name: &str, default: Option<u32>) -> Result<u32, String> {
    match args.get(name).and_then(Json::as_u64) {
        Some(v) => u32::try_from(v).map_err(|_| format!("arg '{name}' = {v} out of u32 range")),
        None => default.ok_or_else(|| format!("missing arg '{name}' (u32)")),
    }
}

fn arg_str_opt<'a>(args: &'a Json, name: &str) -> Option<&'a str> {
    args.get(name).and_then(Json::as_str)
}

fn parse_hwnd(args: &Json) -> Result<Option<isize>, String> {
    let Some(s) = arg_str_opt(args, "hwnd") else {
        return Ok(None);
    };
    let trimmed = s.trim_start_matches("0x").trim_start_matches("0X");
    let v = u64::from_str_radix(trimmed, 16).map_err(|e| format!("bad hwnd '{s}': {e}"))?;
    Ok(Some(v as isize))
}

fn parse_backend(args: &Json) -> Result<Backend, String> {
    match arg_str_opt(args, "backend") {
        Some(s) => Backend::parse(s),
        None => Ok(Backend::default()),
    }
}

fn resolve_l2_hwnd(args: &Json) -> Result<isize, String> {
    if let Some(h) = parse_hwnd(args)? {
        return Ok(h);
    }
    super::foreground_hwnd()
        .ok_or_else(|| "no hwnd given and GetForegroundWindow returned NULL".into())
}

fn parse_mods(args: &Json) -> (bool, bool) {
    let shift = args
        .get("mods")
        .and_then(Json::as_array)
        .map(|a| a.iter().any(|v| v.as_str() == Some("shift")))
        .unwrap_or(false);
    let ctrl = args
        .get("mods")
        .and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .any(|v| v.as_str() == Some("ctrl") || v.as_str() == Some("control"))
        })
        .unwrap_or(false);
    (shift, ctrl)
}

fn l1_axis(axis: Axis, value: f32) -> Result<(), String> {
    match axis {
        Axis::MoveForward => l1_direction_keys(Key(0x57), Key(0x53), value),
        Axis::MoveRight => l1_direction_keys(Key(0x44), Key(0x41), value),
        Axis::MouseX => l1::move_rel(value.round() as i32, 0),
        Axis::MouseY => l1::move_rel(0, value.round() as i32),
    }
}

fn l1_direction_keys(positive: Key, negative: Key, value: f32) -> Result<(), String> {
    if value > 0.0 {
        l1::key_up(negative)?;
        l1::key_down(positive)
    } else if value < 0.0 {
        l1::key_up(positive)?;
        l1::key_down(negative)
    } else {
        l1::key_up(positive)?;
        l1::key_up(negative)
    }
}

/// Every op declared in this module. Consumer mods call
/// `OP_REGISTRY.register_many(modforge::input::ops::all())` at attach.
pub fn all() -> Vec<OpDef> {
    vec![
        OpDef::new(
            "input.player.commands",
            "Send one ordered command batch through the registered in-process player input surface.",
            "{commands: [{kind: axis, axis, value, delta_time}|{kind: key, key, down}]}",
            |args| {
                let commands: Vec<PlayerCommand> = serde_json::from_value(
                    args.get("commands")
                        .cloned()
                        .ok_or("missing arg 'commands' (array)")?,
                )
                .map_err(|error| format!("invalid player command batch: {error}"))?;
                let surface = super::input_surface()
                    .ok_or("no in-process player input surface is registered")?;
                surface.commands(&commands)?;
                Ok(json!({
                    "ok": true,
                    "surface": surface.name(),
                    "commands": commands.len(),
                }))
            },
        ),
        OpDef::new(
            "input.player.pose",
            "Read player position and control yaw through the registered in-process input surface.",
            "{}",
            |_args| {
                let surface = super::input_surface()
                    .ok_or("no in-process player input surface is registered")?;
                Ok(json!({"surface": surface.name(), "pose": surface.pose()?}))
            },
        ),
        OpDef::new(
            "input.mouse.move",
            "Move the mouse to (x, y). L1 = screen px; L2 = client-area px of hwnd.",
            "{x: i32, y: i32, backend?: l1|l2, hwnd?: hex}",
            |args| {
                let x = arg_i32(args, "x")?;
                let y = arg_i32(args, "y")?;
                let backend = parse_backend(args)?;
                match backend {
                    Backend::L1 => l1::move_abs(x, y)?,
                    Backend::L2 => {
                        let h = resolve_l2_hwnd(args)?;
                        l2::move_client(h, x, y)?;
                    }
                    Backend::L3 => {
                        l3_or_fallback("mouse.move", |s| s.move_abs(x, y), || l1::move_abs(x, y))?
                    }
                }
                Ok(json!({"ok": true, "backend": backend, "x": x, "y": y}))
            },
        ),
        OpDef::new(
            "input.mouse.move_rel",
            "Move the mouse by relative device units for camera look.",
            "{dx: i32, dy: i32, backend?: l1|l3}",
            |args| {
                let dx = arg_i32(args, "dx")?;
                let dy = arg_i32(args, "dy")?;
                let backend = parse_backend(args)?;
                match backend {
                    Backend::L1 => l1::move_rel(dx, dy)?,
                    Backend::L2 => {
                        return Err(
                            "relative mouse movement is unavailable through window messages".into(),
                        );
                    }
                    Backend::L3 => l3_or_fallback(
                        "mouse.move_rel",
                        |surface| {
                            surface.axis(Axis::MouseX, dx as f32, 0.0)?;
                            surface.axis(Axis::MouseY, dy as f32, 0.0)
                        },
                        || l1::move_rel(dx, dy),
                    )?,
                }
                Ok(json!({"ok": true, "backend": backend, "dx": dx, "dy": dy}))
            },
        ),
        OpDef::new(
            "input.axis",
            "Submit one player movement or look-axis sample.",
            "{axis: move_forward|move_right|mouse_x|mouse_y, value: f32, delta_time?: f32, backend?: l1|l3}",
            |args| {
                let axis_text = arg_str_opt(args, "axis").ok_or("missing arg 'axis' (str)")?;
                let axis = Axis::parse(axis_text)?;
                let value = args
                    .get("value")
                    .and_then(Json::as_f64)
                    .ok_or("missing arg 'value' (f32)")? as f32;
                let delta_time =
                    args.get("delta_time").and_then(Json::as_f64).unwrap_or(0.0) as f32;
                let backend = parse_backend(args)?;
                match backend {
                    Backend::L1 => l1_axis(axis, value)?,
                    Backend::L2 => {
                        return Err("player axes are unavailable through window messages".into());
                    }
                    Backend::L3 => l3_or_fallback(
                        "axis",
                        |surface| surface.axis(axis, value, delta_time),
                        || l1_axis(axis, value),
                    )?,
                }
                Ok(json!({
                    "ok": true,
                    "backend": backend,
                    "axis": axis,
                    "value": value,
                    "delta_time": delta_time,
                }))
            },
        ),
        OpDef::new(
            "input.mouse.click",
            "Click `button` at (x, y). L1 = screen px; L2 = client-area px of hwnd.",
            "{button: left|right|middle|x1|x2, x: i32, y: i32, backend?: l1|l2, hwnd?: hex, mods?: [shift|ctrl]}",
            |args| {
                let btn_s = arg_str_opt(args, "button").unwrap_or("left");
                let button = Button::parse(btn_s)?;
                let x = arg_i32(args, "x")?;
                let y = arg_i32(args, "y")?;
                let backend = parse_backend(args)?;
                let (shift, ctrl) = parse_mods(args);
                match backend {
                    Backend::L1 => {
                        // For L1 chord-clicks, press the mod keys around the click.
                        if shift {
                            l1::key_down(Key(0xA0))?;
                        }
                        if ctrl {
                            l1::key_down(Key(0xA2))?;
                        }
                        let r = l1::click(button, x, y);
                        if ctrl {
                            let _ = l1::key_up(Key(0xA2));
                        }
                        if shift {
                            let _ = l1::key_up(Key(0xA0));
                        }
                        r?
                    }
                    Backend::L2 => {
                        let h = resolve_l2_hwnd(args)?;
                        let mk_extra = l2::modifier_mask(shift, ctrl);
                        l2::click(h, button, x, y, mk_extra)?;
                    }
                    Backend::L3 => l3_or_fallback(
                        "mouse.click",
                        |s| {
                            if shift {
                                s.key(Key(0xA0), true)?;
                            }
                            if ctrl {
                                s.key(Key(0xA2), true)?;
                            }
                            let r = s.click(button, x, y);
                            if ctrl {
                                let _ = s.key(Key(0xA2), false);
                            }
                            if shift {
                                let _ = s.key(Key(0xA0), false);
                            }
                            r
                        },
                        || {
                            if shift {
                                l1::key_down(Key(0xA0))?;
                            }
                            if ctrl {
                                l1::key_down(Key(0xA2))?;
                            }
                            let r = l1::click(button, x, y);
                            if ctrl {
                                let _ = l1::key_up(Key(0xA2));
                            }
                            if shift {
                                let _ = l1::key_up(Key(0xA0));
                            }
                            r
                        },
                    )?,
                }
                Ok(json!({"ok": true, "backend": backend, "button": btn_s, "x": x, "y": y}))
            },
        ),
        OpDef::new(
            "input.key.down",
            "Press a key (no release).",
            "{key: str, backend?: l1|l2, hwnd?: hex}",
            |args| {
                let key_s = arg_str_opt(args, "key").ok_or("missing arg 'key' (str)")?;
                let key = Key::parse(key_s)?;
                let backend = parse_backend(args)?;
                match backend {
                    Backend::L1 => l1::key_down(key)?,
                    Backend::L2 => l2::key_down(resolve_l2_hwnd(args)?, key)?,
                    Backend::L3 => {
                        l3_or_fallback("key.down", |s| s.key(key, true), || l1::key_down(key))?
                    }
                }
                Ok(json!({"ok": true, "backend": backend, "key": key_s, "vk": key.0}))
            },
        ),
        OpDef::new(
            "input.key.up",
            "Release a key.",
            "{key: str, backend?: l1|l2, hwnd?: hex}",
            |args| {
                let key_s = arg_str_opt(args, "key").ok_or("missing arg 'key' (str)")?;
                let key = Key::parse(key_s)?;
                let backend = parse_backend(args)?;
                match backend {
                    Backend::L1 => l1::key_up(key)?,
                    Backend::L2 => l2::key_up(resolve_l2_hwnd(args)?, key)?,
                    Backend::L3 => {
                        l3_or_fallback("key.up", |s| s.key(key, false), || l1::key_up(key))?
                    }
                }
                Ok(json!({"ok": true, "backend": backend, "key": key_s, "vk": key.0}))
            },
        ),
        OpDef::new(
            "input.key.press",
            "Press + release a key. Optional `hold_ms` sleeps between down and up.",
            "{key: str, hold_ms?: u32, backend?: l1|l2, hwnd?: hex}",
            |args| {
                let key_s = arg_str_opt(args, "key").ok_or("missing arg 'key' (str)")?;
                let key = Key::parse(key_s)?;
                let hold_ms = arg_u32(args, "hold_ms", Some(0))?;
                let backend = parse_backend(args)?;
                match backend {
                    Backend::L1 => l1::key_press(key, hold_ms)?,
                    Backend::L2 => l2::key_press(resolve_l2_hwnd(args)?, key, hold_ms)?,
                    Backend::L3 => l3_or_fallback(
                        "key.press",
                        |s| {
                            s.key(key, true)?;
                            if hold_ms > 0 {
                                std::thread::sleep(std::time::Duration::from_millis(
                                    hold_ms as u64,
                                ));
                            }
                            s.key(key, false)
                        },
                        || l1::key_press(key, hold_ms),
                    )?,
                }
                Ok(json!({
                    "ok": true, "backend": backend, "key": key_s, "vk": key.0, "hold_ms": hold_ms
                }))
            },
        ),
        OpDef::new(
            "input.cursor.get",
            "Return the current OS cursor position in screen pixels (GetCursorPos).",
            "",
            |_args| {
                let (x, y) = l1::cursor_pos()?;
                Ok(json!({"x": x, "y": y}))
            },
        ),
        OpDef::new(
            "input.foreground.hwnd",
            "Return the foreground window's HWND (hex). Useful for L2 callers.",
            "",
            |_args| match super::foreground_hwnd() {
                Some(h) => Ok(json!({"hwnd": format!("0x{:x}", h as usize)})),
                None => Err("GetForegroundWindow returned NULL".into()),
            },
        ),
        OpDef::new(
            "input.find_hwnd_by_pid",
            "Return the first visible top-level HWND owned by `pid`. Hex string.",
            "{pid: u32}",
            |args| {
                let pid = arg_u32(args, "pid", None)?;
                match super::find_hwnd_by_pid(pid) {
                    Some(h) => Ok(json!({"hwnd": format!("0x{:x}", h as usize), "pid": pid})),
                    None => Err(format!("no visible top-level window for pid {pid}")),
                }
            },
        ),
        OpDef::new(
            "input.mouse.drag",
            "Drag `button` from (x1,y1) to (x2,y2). Generates `steps` intermediate \
moves over `duration_ms`. L1 = screen px; L2 = client-area px of hwnd.",
            "{button: left|right|middle, x1: i32, y1: i32, x2: i32, y2: i32, \
duration_ms?: u32, steps?: u32, backend?: l1|l2, hwnd?: hex, mods?: [shift|ctrl]}",
            |args| {
                let btn_s = arg_str_opt(args, "button").unwrap_or("left");
                let button = Button::parse(btn_s)?;
                let x1 = arg_i32(args, "x1")?;
                let y1 = arg_i32(args, "y1")?;
                let x2 = arg_i32(args, "x2")?;
                let y2 = arg_i32(args, "y2")?;
                let duration_ms = arg_u32(args, "duration_ms", Some(150))?;
                let steps = arg_u32(args, "steps", Some(16))?;
                let backend = parse_backend(args)?;
                let (shift, ctrl) = parse_mods(args);
                match backend {
                    Backend::L1 => {
                        if shift {
                            l1::key_down(Key(0xA0))?;
                        }
                        if ctrl {
                            l1::key_down(Key(0xA2))?;
                        }
                        let r = l1::drag(button, x1, y1, x2, y2, duration_ms, steps);
                        if ctrl {
                            let _ = l1::key_up(Key(0xA2));
                        }
                        if shift {
                            let _ = l1::key_up(Key(0xA0));
                        }
                        r?
                    }
                    Backend::L2 => {
                        let h = resolve_l2_hwnd(args)?;
                        let mk_extra = l2::modifier_mask(shift, ctrl);
                        l2::drag(h, button, x1, y1, x2, y2, duration_ms, steps, mk_extra)?;
                    }
                    Backend::L3 => {
                        // No L3 drag trait method yet; the surface uses
                        // move_abs + simulated press/release.
                        l3_or_fallback(
                            "mouse.drag",
                            |s| {
                                if shift {
                                    s.key(Key(0xA0), true)?;
                                }
                                if ctrl {
                                    s.key(Key(0xA2), true)?;
                                }
                                s.move_abs(x1, y1)?;
                                s.click(button, x1, y1)?; // best-effort: single click sequence
                                s.move_abs(x2, y2)?;
                                if ctrl {
                                    let _ = s.key(Key(0xA2), false);
                                }
                                if shift {
                                    let _ = s.key(Key(0xA0), false);
                                }
                                Ok(())
                            },
                            || l1::drag(button, x1, y1, x2, y2, duration_ms, steps),
                        )?
                    }
                }
                Ok(json!({
                    "ok": true, "backend": backend, "button": btn_s,
                    "from": [x1, y1], "to": [x2, y2],
                    "duration_ms": duration_ms, "steps": steps,
                }))
            },
        ),
        OpDef::new(
            "input.mouse.scroll",
            "Scroll the wheel by `dy` ticks (positive = up). `dx` for horizontal. \
L2 requires `hwnd` + `x` + `y` (screen px); L1 scrolls at current cursor.",
            "{dy?: i32, dx?: i32, x?: i32, y?: i32, backend?: l1|l2, hwnd?: hex}",
            |args| {
                let dy = args
                    .get("dy")
                    .and_then(Json::as_i64)
                    .map(|v| v as i32)
                    .unwrap_or(0);
                let dx = args
                    .get("dx")
                    .and_then(Json::as_i64)
                    .map(|v| v as i32)
                    .unwrap_or(0);
                if dx == 0 && dy == 0 {
                    return Err("scroll: at least one of dx, dy must be nonzero".into());
                }
                let backend = parse_backend(args)?;
                match backend {
                    Backend::L1 => l1::scroll(dx, dy)?,
                    Backend::L2 => {
                        let h = resolve_l2_hwnd(args)?;
                        let x = args.get("x").and_then(Json::as_i64).unwrap_or(0) as i32;
                        let y = args.get("y").and_then(Json::as_i64).unwrap_or(0) as i32;
                        l2::scroll(h, x, y, dx, dy)?;
                    }
                    // No L3 scroll trait method yet; always falls back.
                    Backend::L3 => l3_or_fallback(
                        "mouse.scroll",
                        |_s| l1::scroll(dx, dy),
                        || l1::scroll(dx, dy),
                    )?,
                }
                Ok(json!({"ok": true, "backend": backend, "dx": dx, "dy": dy}))
            },
        ),
        OpDef::new(
            "input.combo",
            "Hold `keys` (modifiers or any key list) while invoking `then`. \
`then` is another input op envelope: `{op: 'input.mouse.click', args: {...}}` \
or `{op: 'input.key.press', args: {...}}`. Backend on the inner op picks L1/L2; \
combo runs the modifier presses through the SAME backend.",
            "{keys: [str], then: {op: str, args: {...}}}",
            |args| {
                let keys_arr = args
                    .get("keys")
                    .and_then(Json::as_array)
                    .ok_or("missing arg 'keys' (array of str)")?;
                let mut keys: Vec<Key> = Vec::with_capacity(keys_arr.len());
                for (i, k) in keys_arr.iter().enumerate() {
                    let s = k.as_str().ok_or_else(|| format!("keys[{i}] not str"))?;
                    keys.push(Key::parse(s)?);
                }
                let then = args.get("then").ok_or("missing arg 'then'")?;
                let inner_op = then
                    .get("op")
                    .and_then(Json::as_str)
                    .ok_or("then.op missing")?;
                let inner_args = then.get("args").cloned().unwrap_or(json!({}));
                let backend = parse_backend(&inner_args)?;

                // Press modifiers through the chosen backend.
                let hwnd_for_l2 = if backend == Backend::L2 {
                    Some(resolve_l2_hwnd(&inner_args)?)
                } else {
                    None
                };
                for k in &keys {
                    match backend {
                        Backend::L1 => l1::key_down(*k)?,
                        Backend::L2 => l2::key_down(hwnd_for_l2.unwrap(), *k)?,
                        Backend::L3 => l3_or_fallback(
                            "combo.key.down",
                            |s| s.key(*k, true),
                            || l1::key_down(*k),
                        )?,
                    }
                }
                // Dispatch the inner op via the registry. Sibling-call
                // protection: if dispatch panics or errors, we still
                // release the modifiers below.
                let dispatched = crate::ops::OP_REGISTRY.dispatch(inner_op, &inner_args);
                // Release modifiers in REVERSE order (LIFO).
                for k in keys.iter().rev() {
                    let _ = match backend {
                        Backend::L1 => l1::key_up(*k),
                        Backend::L2 => l2::key_up(hwnd_for_l2.unwrap(), *k),
                        Backend::L3 => {
                            l3_or_fallback("combo.key.up", |s| s.key(*k, false), || l1::key_up(*k))
                        }
                    };
                }
                match dispatched {
                    Some(Ok(inner_result)) => Ok(json!({
                        "ok": true, "combo_keys": keys_arr, "inner": inner_result,
                    })),
                    Some(Err(e)) => Err(format!("inner op '{inner_op}' failed: {e}")),
                    None => Err(format!("inner op '{inner_op}' not registered")),
                }
            },
        ),
        OpDef::new(
            "input.self.hwnd",
            "Return the first visible top-level HWND owned by the current process. \
This is the in-process shortcut for tests / cmdlets running INSIDE the game.",
            "",
            |_args| {
                use windows_sys::Win32::System::Threading::GetCurrentProcessId;
                let pid = unsafe { GetCurrentProcessId() };
                match super::find_hwnd_by_pid(pid) {
                    Some(h) => Ok(json!({"hwnd": format!("0x{:x}", h as usize), "pid": pid})),
                    None => Err(format!("no visible top-level window for self pid {pid}")),
                }
            },
        ),
    ]
}
