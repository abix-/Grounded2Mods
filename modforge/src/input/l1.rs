//! L1 backend: OS-global synthetic input via `SendInput`.
//!
//! Mirrors enigo/winput/rdev shape but hand-rolled over the existing
//! `windows-sys` dep so modforge picks up zero new crates.
//!
//! All events are tagged with [`super::SYNTHETIC_EXTRA_INFO`] so a
//! future input listener can ignore our own synthesized events.
//!
//! Coordinate space: `move_abs` accepts physical screen pixels in the
//! 0..65535 normalized space required by `MOUSEEVENTF_ABSOLUTE`; the
//! conversion is internal.

use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, MapVirtualKeyW,
    SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};

use super::{Button, Key, SYNTHETIC_EXTRA_INFO};

fn send(input: &INPUT) -> Result<(), String> {
    let n = unsafe { SendInput(1, input as *const INPUT, size_of::<INPUT>() as i32) };
    if n == 1 {
        Ok(())
    } else {
        let err = std::io::Error::last_os_error();
        Err(format!("SendInput returned {n} (last_os_error = {err})"))
    }
}

fn screen_to_normalized(x: i32, y: i32) -> (i32, i32) {
    // MOUSEEVENTF_ABSOLUTE expects 0..65535 normalized to the virtual
    // screen (multi-monitor aware via MOUSEEVENTF_VIRTUALDESK).
    let vx = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let vy = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let vw = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) }.max(1);
    let vh = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) }.max(1);
    let nx = ((x - vx) as i64 * 65535 / vw as i64) as i32;
    let ny = ((y - vy) as i64 * 65535 / vh as i64) as i32;
    (nx, ny)
}

/// Read the current OS cursor position in screen pixels.
pub fn cursor_pos() -> Result<(i32, i32), String> {
    let mut p = POINT { x: 0, y: 0 };
    let ok = unsafe { GetCursorPos(&mut p) };
    if ok != 0 {
        Ok((p.x, p.y))
    } else {
        Err(format!("GetCursorPos failed: {}", std::io::Error::last_os_error()))
    }
}

/// Move the cursor to absolute screen coordinates.
pub fn move_abs(x: i32, y: i32) -> Result<(), String> {
    let (nx, ny) = screen_to_normalized(x, y);
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: nx,
                dy: ny,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                time: 0,
                dwExtraInfo: SYNTHETIC_EXTRA_INFO,
            },
        },
    };
    send(&input)
}

/// Move the mouse by relative device units. First-person games consume these
/// as look-axis input rather than as a cursor position.
pub fn move_rel(dx: i32, dy: i32) -> Result<(), String> {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE,
                time: 0,
                dwExtraInfo: SYNTHETIC_EXTRA_INFO,
            },
        },
    };
    send(&input)
}

fn button_flags(b: Button, down: bool) -> (u32, u32) {
    match b {
        Button::Left => (
            if down { MOUSEEVENTF_LEFTDOWN } else { MOUSEEVENTF_LEFTUP },
            0,
        ),
        Button::Right => (
            if down { MOUSEEVENTF_RIGHTDOWN } else { MOUSEEVENTF_RIGHTUP },
            0,
        ),
        Button::Middle => (
            if down { MOUSEEVENTF_MIDDLEDOWN } else { MOUSEEVENTF_MIDDLEUP },
            0,
        ),
        Button::XButton1 => (
            if down { MOUSEEVENTF_XDOWN } else { MOUSEEVENTF_XUP },
            0x0001,
        ),
        Button::XButton2 => (
            if down { MOUSEEVENTF_XDOWN } else { MOUSEEVENTF_XUP },
            0x0002,
        ),
    }
}

fn mouse_button_event(b: Button, down: bool) -> Result<(), String> {
    let (flags, data) = button_flags(b, down);
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: SYNTHETIC_EXTRA_INFO,
            },
        },
    };
    send(&input)
}

/// Click `button` at `(x, y)` screen pixels. Moves cursor first, then
/// presses + releases. The 1ms gap is omitted; most games sample
/// state once per frame so back-to-back down/up still register.
pub fn click(button: Button, x: i32, y: i32) -> Result<(), String> {
    click_held(button, x, y, 60)
}

/// `click` with an explicit down-to-up hold in ms. The default 60ms
/// (used by `click()`) is enough to be observed across ~3 frames at
/// 60 fps. Many games' input pumps poll `GetAsyncKeyState` once per
/// frame and would miss a 0ms press entirely. Clamped to 0..=5000.
pub fn click_held(button: Button, x: i32, y: i32, hold_ms: u32) -> Result<(), String> {
    move_abs(x, y)?;
    mouse_button_event(button, true)?;
    let hold = hold_ms.min(5000);
    if hold > 0 {
        std::thread::sleep(std::time::Duration::from_millis(hold as u64));
    }
    mouse_button_event(button, false)?;
    Ok(())
}

/// Drag `button` from `(fx, fy)` to `(tx, ty)`. Generates `steps`
/// intermediate `move_abs` events over `duration_ms` so the game's
/// per-frame mouse-sample sees motion rather than teleport. `steps`
/// is clamped to `1..=256`; `duration_ms` is clamped to `0..=10_000`.
pub fn drag(
    button: Button,
    fx: i32,
    fy: i32,
    tx: i32,
    ty: i32,
    duration_ms: u32,
    steps: u32,
) -> Result<(), String> {
    let steps = steps.clamp(1, 256);
    let duration_ms = duration_ms.min(10_000);
    let sleep_per = if steps > 1 {
        std::time::Duration::from_millis((duration_ms as u64) / (steps as u64 - 1).max(1))
    } else {
        std::time::Duration::ZERO
    };

    move_abs(fx, fy)?;
    mouse_button_event(button, true)?;
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let x = fx + ((tx - fx) as f64 * t).round() as i32;
        let y = fy + ((ty - fy) as f64 * t).round() as i32;
        move_abs(x, y)?;
        if i < steps && !sleep_per.is_zero() {
            std::thread::sleep(sleep_per);
        }
    }
    mouse_button_event(button, false)?;
    Ok(())
}

/// One wheel-tick worth of `MOUSEEVENTF_*WHEEL` data.
const WHEEL_DELTA: i32 = 120;

/// Vertical / horizontal scroll. `dy > 0` is "scroll up / away from user".
/// `dx > 0` is "scroll right". Values are in wheel-ticks (one notch each).
pub fn scroll(dx: i32, dy: i32) -> Result<(), String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MOUSEEVENTF_HWHEEL, MOUSEEVENTF_WHEEL,
    };
    if dy != 0 {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: (dy * WHEEL_DELTA) as u32,
                    dwFlags: MOUSEEVENTF_WHEEL,
                    time: 0,
                    dwExtraInfo: SYNTHETIC_EXTRA_INFO,
                },
            },
        };
        send(&input)?;
    }
    if dx != 0 {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: (dx * WHEEL_DELTA) as u32,
                    dwFlags: MOUSEEVENTF_HWHEEL,
                    time: 0,
                    dwExtraInfo: SYNTHETIC_EXTRA_INFO,
                },
            },
        };
        send(&input)?;
    }
    Ok(())
}

fn key_event(key: Key, down: bool) -> Result<(), String> {
    let scan = unsafe { MapVirtualKeyW(key.0 as u32, 0 /* MAPVK_VK_TO_VSC */) } as u16;
    let mut flags = KEYEVENTF_SCANCODE;
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: 0,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: SYNTHETIC_EXTRA_INFO,
            },
        },
    };
    send(&input)
}

/// Press a key (no release).
pub fn key_down(key: Key) -> Result<(), String> {
    key_event(key, true)
}

/// Release a key (no press).
pub fn key_up(key: Key) -> Result<(), String> {
    key_event(key, false)
}

/// Press + release. `hold_ms` (if > 0) sleeps between down and up.
pub fn key_press(key: Key, hold_ms: u32) -> Result<(), String> {
    key_event(key, true)?;
    if hold_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(hold_ms as u64));
    }
    key_event(key, false)
}
