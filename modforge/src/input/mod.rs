//! Synthetic input: mouse + keyboard.
//!
//! Three-layer architecture (validated by prior art in
//! [`../../docs/input-prior-art.md`](../../docs/input-prior-art.md)):
//!
//! - **L1** (`l1::send_input`): OS-global `SendInput`. Goes through the
//!   Windows raw-input pipeline; works on every app but steals focus
//!   and requires the target window to be foreground.
//! - **L2** (`l2::window_message`): per-window `PostMessage` /
//!   `SendMessage` to a target HWND. No focus theft, works on hidden
//!   windows. Some games (raw-input only) ignore these messages.
//! - **L3** (`InputSurface` trait): per-game internal pokes. Consumer
//!   mods (horsey-mod, grounded2-mod, ...) implement this trait against
//!   their engine's input subsystem (UE `APlayerController::InputKey`,
//!   Unity `Input.GetKey*` hook, raw-Win32 mouse-state poke).
//!
//! First-slice scope: L1 + L2 backends, mouse move/click, keyboard
//! down/up/press. Drag/scroll/combo/replay deferred. The HTTP cmdlets
//! in [`ops`] expose these primitives over the modforge op registry.

pub mod l1;
pub mod l2;
pub mod ops;

use serde::{Deserialize, Serialize};

/// Which input dispatch path to use for one call.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    /// `SendInput` (OS-global). Default.
    #[default]
    L1,
    /// `PostMessage`/`SendMessage` to a target HWND.
    L2,
    /// Game-internal poke via the registered [`InputSurface`].
    /// Falls back to L1 with a logged warning if no surface is
    /// registered.
    L3,
}

impl Backend {
    pub fn parse(s: &str) -> Result<Backend, String> {
        match s.to_ascii_lowercase().as_str() {
            "l1" | "sendinput" | "send_input" => Ok(Backend::L1),
            "l2" | "postmessage" | "post_message" | "windowmessage" => Ok(Backend::L2),
            "l3" | "gameinternal" | "game_internal" | "surface" => Ok(Backend::L3),
            other => Err(format!(
                "unknown input backend '{other}' (expected l1|l2|l3)"
            )),
        }
    }
}

/// Logical mouse buttons. Maps to Win32 `MOUSEEVENTF_*` / `MK_*`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Button {
    Left,
    Right,
    Middle,
    XButton1,
    XButton2,
}

/// Relative look axes used by first-person and third-person cameras.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    MouseX,
    MouseY,
}

impl Axis {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "mouse_x" | "mousex" | "x" => Ok(Self::MouseX),
            "mouse_y" | "mousey" | "y" => Ok(Self::MouseY),
            other => Err(format!(
                "unknown input axis '{other}' (expected mouse_x|mouse_y)"
            )),
        }
    }
}

impl Button {
    pub fn parse(s: &str) -> Result<Button, String> {
        match s.to_ascii_lowercase().as_str() {
            "left" | "l" | "lmb" => Ok(Button::Left),
            "right" | "r" | "rmb" => Ok(Button::Right),
            "middle" | "m" | "mmb" => Ok(Button::Middle),
            "x1" | "xbutton1" => Ok(Button::XButton1),
            "x2" | "xbutton2" => Ok(Button::XButton2),
            other => Err(format!(
                "unknown mouse button '{other}' (expected left|right|middle|x1|x2)"
            )),
        }
    }
}

/// Virtual-key code (Windows VK_*). 0..=0xFF.
///
/// Construct via [`Key::parse`] (accepts friendly names like `"shift"`,
/// `"a"`, `"f4"`, or a hex like `"0x10"`).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Key(pub u16);

impl Key {
    /// Friendly-name parser. Recognized:
    /// - Modifiers: `shift`/`ctrl`/`control`/`alt`/`menu`/`win`/`lwin`/`rwin`
    /// - Whitespace: `space`/`tab`/`enter`/`return`/`backspace`/`esc`/`escape`
    /// - Navigation: `up`/`down`/`left`/`right`/`home`/`end`/`pageup`/`pagedown`/`insert`/`delete`
    /// - Letters: `a`..=`z` (case insensitive) -> VK_A..VK_Z (0x41..0x5A)
    /// - Digits: `0`..=`9` -> VK_0..VK_9 (0x30..0x39)
    /// - Function: `f1`..=`f24` -> VK_F1..VK_F24 (0x70..0x87)
    /// - Numeric: leading `0x` parsed as hex; otherwise as decimal
    pub fn parse(s: &str) -> Result<Key, String> {
        let s = s.trim();
        let lower = s.to_ascii_lowercase();
        let vk: u16 = match lower.as_str() {
            "shift" | "lshift" => 0xA0,
            "rshift" => 0xA1,
            "ctrl" | "control" | "lctrl" | "lcontrol" => 0xA2,
            "rctrl" | "rcontrol" => 0xA3,
            "alt" | "menu" | "lalt" | "lmenu" => 0xA4,
            "ralt" | "rmenu" => 0xA5,
            "win" | "lwin" | "lsuper" | "meta" => 0x5B,
            "rwin" | "rsuper" => 0x5C,
            "space" => 0x20,
            "tab" => 0x09,
            "enter" | "return" => 0x0D,
            "backspace" | "back" => 0x08,
            "esc" | "escape" => 0x1B,
            "up" => 0x26,
            "down" => 0x28,
            "left" => 0x25,
            "right" => 0x27,
            "home" => 0x24,
            "end" => 0x23,
            "pageup" | "pgup" => 0x21,
            "pagedown" | "pgdn" => 0x22,
            "insert" | "ins" => 0x2D,
            "delete" | "del" => 0x2E,
            "capslock" => 0x14,
            _ => {
                if let Some(hex) = lower.strip_prefix("0x") {
                    u16::from_str_radix(hex, 16)
                        .map_err(|e| format!("bad hex vk '{s}': {e}"))?
                } else if let Some(rest) = lower.strip_prefix('f') {
                    let n: u16 = rest
                        .parse()
                        .map_err(|_| format!("bad function key '{s}'"))?;
                    if !(1..=24).contains(&n) {
                        return Err(format!("function key {n} out of range 1..=24"));
                    }
                    0x70 + (n - 1)
                } else if lower.len() == 1 {
                    let c = lower.chars().next().unwrap();
                    if c.is_ascii_alphabetic() {
                        0x41 + (c as u16 - 'a' as u16)
                    } else if c.is_ascii_digit() {
                        0x30 + (c as u16 - '0' as u16)
                    } else {
                        return Err(format!("unrecognized single-char key '{s}'"));
                    }
                } else {
                    lower.parse::<u16>()
                        .map_err(|_| format!("unrecognized key name '{s}'"))?
                }
            }
        };
        if vk > 0xFF {
            return Err(format!("vk {vk:#x} > 0xFF"));
        }
        Ok(Key(vk))
    }
}

/// Per-game L3 input surface. Consumer mods implement this against
/// their engine's input subsystem; modforge registers it as the
/// optional L3 backend.
pub trait InputSurface: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn click(&self, button: Button, x: i32, y: i32) -> Result<(), String>;
    fn move_abs(&self, x: i32, y: i32) -> Result<(), String>;
    fn key(&self, key: Key, down: bool) -> Result<(), String>;
    fn axis(&self, axis: Axis, value: f32, delta_time: f32) -> Result<(), String>;
}

/// Global slot for the registered per-game [`InputSurface`]. First
/// registration wins; subsequent `set_input_surface` calls are
/// silently ignored to keep idempotent re-init (hot-reload) safe.
static INPUT_SURFACE: parking_lot::Mutex<Option<&'static dyn InputSurface>> =
    parking_lot::Mutex::new(None);

/// Register the L3 input surface for this consumer mod. Call once
/// at attach. The surface is leaked into `'static` storage so it
/// can be looked up by op handlers without ownership tracking;
/// this matches how `OP_REGISTRY` handlers work.
pub fn set_input_surface<S: InputSurface>(surface: S) {
    let boxed: &'static dyn InputSurface = Box::leak(Box::new(surface));
    let mut g = INPUT_SURFACE.lock();
    if g.is_none() {
        *g = Some(boxed);
    }
}

/// Look up the registered L3 input surface.
pub fn input_surface() -> Option<&'static dyn InputSurface> {
    *INPUT_SURFACE.lock()
}

/// Tag stamped on `INPUT.mi.dwExtraInfo` / `ki.dwExtraInfo` for L1
/// events so future input listeners can recognize self-sent events.
/// Random nonzero byte string ("modforge synthetic input").
pub const SYNTHETIC_EXTRA_INFO: usize = 0x6D_6F_64_66_69_6E_70_75;

/// Get the foreground window's HWND as a `*mut std::ffi::c_void` (the
/// `windows-sys` HWND type). Used by L2 as the default target when no
/// explicit HWND was supplied. Returns `None` if there is no
/// foreground window (rare, but possible during early process boot).
pub fn foreground_hwnd() -> Option<isize> {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let h = unsafe { GetForegroundWindow() };
    if h.is_null() { None } else { Some(h as isize) }
}

/// Enumerate top-level windows; return the first one owned by `pid`
/// that is visible. Useful when the test process and the game process
/// are separate: pass the game PID, get the game window.
///
/// Visibility filter: skips invisible windows (typical message-only
/// or hidden helper windows). If multiple visible top-level windows
/// belong to the same PID (rare for games), returns the first
/// EnumWindows yields.
pub fn find_hwnd_by_pid(pid: u32) -> Option<isize> {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
    };
    type BOOL = i32;

    struct Ctx {
        wanted_pid: u32,
        found: isize,
    }

    extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = unsafe { &mut *(lparam as *mut c_void as *mut Ctx) };
        let mut pid: u32 = 0;
        let _tid = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == ctx.wanted_pid && unsafe { IsWindowVisible(hwnd) } != 0 {
            ctx.found = hwnd as isize;
            return 0; // stop enumeration
        }
        1 // continue
    }

    let mut ctx = Ctx { wanted_pid: pid, found: 0 };
    let _ok = unsafe {
        EnumWindows(
            Some(cb),
            &mut ctx as *mut Ctx as *mut c_void as LPARAM,
        )
    };
    if ctx.found == 0 { None } else { Some(ctx.found) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_parse_recognized() {
        assert_eq!(Backend::parse("l1").unwrap(), Backend::L1);
        assert_eq!(Backend::parse("L1").unwrap(), Backend::L1);
        assert_eq!(Backend::parse("SendInput").unwrap(), Backend::L1);
        assert_eq!(Backend::parse("send_input").unwrap(), Backend::L1);
        assert_eq!(Backend::parse("l2").unwrap(), Backend::L2);
        assert_eq!(Backend::parse("postmessage").unwrap(), Backend::L2);
        assert_eq!(Backend::parse("WindowMessage").unwrap(), Backend::L2);
        assert_eq!(Backend::parse("l3").unwrap(), Backend::L3);
        assert_eq!(Backend::parse("game_internal").unwrap(), Backend::L3);
    }

    #[test]
    fn backend_parse_rejects_garbage() {
        assert!(Backend::parse("").is_err());
        assert!(Backend::parse("foo").is_err());
    }

    #[test]
    fn axis_parse() {
        assert_eq!(Axis::parse("mouse_x").unwrap(), Axis::MouseX);
        assert_eq!(Axis::parse("MouseY").unwrap(), Axis::MouseY);
        assert!(Axis::parse("throttle").is_err());
    }

    #[test]
    fn button_parse() {
        assert_eq!(Button::parse("left").unwrap(), Button::Left);
        assert_eq!(Button::parse("L").unwrap(), Button::Left);
        assert_eq!(Button::parse("lmb").unwrap(), Button::Left);
        assert_eq!(Button::parse("right").unwrap(), Button::Right);
        assert_eq!(Button::parse("middle").unwrap(), Button::Middle);
        assert_eq!(Button::parse("x1").unwrap(), Button::XButton1);
        assert_eq!(Button::parse("xbutton2").unwrap(), Button::XButton2);
        assert!(Button::parse("derp").is_err());
    }

    #[test]
    fn key_parse_modifiers() {
        assert_eq!(Key::parse("shift").unwrap().0, 0xA0);
        assert_eq!(Key::parse("ctrl").unwrap().0, 0xA2);
        assert_eq!(Key::parse("alt").unwrap().0, 0xA4);
        assert_eq!(Key::parse("rshift").unwrap().0, 0xA1);
    }

    #[test]
    fn key_parse_letters_digits() {
        assert_eq!(Key::parse("a").unwrap().0, 0x41);
        assert_eq!(Key::parse("Z").unwrap().0, 0x5A);
        assert_eq!(Key::parse("0").unwrap().0, 0x30);
        assert_eq!(Key::parse("9").unwrap().0, 0x39);
    }

    #[test]
    fn key_parse_function_keys() {
        assert_eq!(Key::parse("f1").unwrap().0, 0x70);
        assert_eq!(Key::parse("F12").unwrap().0, 0x7B);
        assert_eq!(Key::parse("f24").unwrap().0, 0x87);
        assert!(Key::parse("f0").is_err());
        assert!(Key::parse("f25").is_err());
        assert!(Key::parse("foo").is_err());
    }

    #[test]
    fn key_parse_navigation() {
        assert_eq!(Key::parse("space").unwrap().0, 0x20);
        assert_eq!(Key::parse("enter").unwrap().0, 0x0D);
        assert_eq!(Key::parse("return").unwrap().0, 0x0D);
        assert_eq!(Key::parse("esc").unwrap().0, 0x1B);
        assert_eq!(Key::parse("escape").unwrap().0, 0x1B);
        assert_eq!(Key::parse("up").unwrap().0, 0x26);
        assert_eq!(Key::parse("pageup").unwrap().0, 0x21);
    }

    #[test]
    fn key_parse_hex() {
        assert_eq!(Key::parse("0x10").unwrap().0, 0x10);
        assert_eq!(Key::parse("0xff").unwrap().0, 0xFF);
        assert!(Key::parse("0x100").is_err()); // > 0xFF
    }
}
