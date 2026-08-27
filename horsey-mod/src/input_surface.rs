//! L3 `InputSurface` impl for Horsey. Writes the engine's
//! cursor-float fields (`LOC+0x174`/`+0x178`) directly so synthetic
//! input doesn't depend on the engine's per-frame Win32 input pump
//! (which polls `GetCursorPos` + `GetAsyncKeyState`).
//!
//! v1 scope (this file):
//! - `move_abs`: write LOC cursor floats. Coordinates are SCREEN
//!   pixels; we run `ScreenToClient` against the game HWND inside.
//! - `click`: move + L1 button event. L3 button writes via the
//!   engine's mouse-state struct (`FUN_14018c5c0()`) are deferred
//!   until that accessor is pattern-resolved; see
//!   [`docs/input-surface.md`](../docs/input-surface.md) §"Surface 2".
//! - `key`: deferred. Modifier flags live at `FUN_140183330(0)+0xe1`
//!   etc.; non-modifier keys route through `FUN_140183990`.
//!   Surface 3 in the design doc.
//!
//! Registered at attach via
//! `modforge::input::set_input_surface(HorseyInputSurface)` so any
//! `input.*` cmdlet with `backend: "l3"` lands here.

use modforge::input::{Axis, Button, InputSurface, Key};
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;

use crate::hk1;

pub struct HorseyInputSurface;

impl HorseyInputSurface {
    pub fn new() -> Self { Self }
}

fn screen_to_client_self(x: i32, y: i32) -> Result<(i32, i32), String> {
    let hwnd = modforge::input::find_hwnd_by_pid(unsafe {
        windows_sys::Win32::System::Threading::GetCurrentProcessId()
    })
    .ok_or("no visible top-level window for self pid (game not yet windowed?)")?;
    let mut p = POINT { x, y };
    let ok = unsafe { ScreenToClient(hwnd as _, &mut p) };
    if ok == 0 {
        return Err(format!(
            "ScreenToClient failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok((p.x, p.y))
}

/// Try to write the LOC cursor floats. Returns `true` on success,
/// `false` if the home location is unresolved (no save loaded). The
/// caller has already done the L1 OS-cursor move, so a false return
/// is "L3 degraded to L1, not an error."
fn try_write_loc_cursor(client_x: f32, client_y: f32) -> bool {
    let Some(loc) = hk1::home_loc_ptr() else {
        return false;
    };
    // SAFETY: home_loc_ptr returned a non-null pointer that it has
    // already verified is readable through at least loc+0x300; the
    // cursor fields live at 0x174 / 0x178.
    unsafe {
        *((loc + hk1::LOC_CURSOR_X) as *mut f32) = client_x;
        *((loc + hk1::LOC_CURSOR_Y) as *mut f32) = client_y;
    }
    true
}

impl InputSurface for HorseyInputSurface {
    fn name(&self) -> &'static str { "horsey" }

    fn move_abs(&self, x: i32, y: i32) -> Result<(), String> {
        // Write OS cursor first. This keeps the user's visible cursor
        // in sync AND feeds the engine's `GetCursorPos` consumer
        // alongside our LOC-floats write.
        modforge::input::l1::move_abs(x, y)?;
        // Convert screen px to client px for the LOC fields. If the
        // game window doesn't exist yet (early boot), drop back to
        // L1-only and report ok.
        let Ok((cx, cy)) = screen_to_client_self(x, y) else {
            return Ok(());
        };
        if !try_write_loc_cursor(cx as f32, cy as f32) {
            // No save loaded; L3 degrades to L1. Quiet log because
            // this is expected at the main menu.
            modforge::log!(
                "[l3 input] LOC unresolved; cursor moved via L1 only ({x},{y})"
            );
        }
        Ok(())
    }

    fn click(&self, button: Button, x: i32, y: i32) -> Result<(), String> {
        // v1: position via L3, button via L1 SendInput. The engine
        // reads VK_LBUTTON/RBUTTON via GetAsyncKeyState, which is
        // updated by SendInput, so this is sufficient for the
        // current pump. Direct mouse-state struct writes deferred
        // (see docs/input-surface.md).
        self.move_abs(x, y)?;
        modforge::input::l1::click(button, x, y)
    }

    fn key(&self, key: Key, down: bool) -> Result<(), String> {
        // v1 fallback: SendInput. Direct engine-keyboard-buffer
        // writes deferred (see docs/input-surface.md §"Surface 3").
        if down {
            modforge::input::l1::key_down(key)
        } else {
            modforge::input::l1::key_up(key)
        }
    }

    fn axis(&self, axis: Axis, value: f32, _delta_time: f32) -> Result<(), String> {
        let delta = value.round() as i32;
        match axis {
            Axis::MoveForward | Axis::MoveRight => Err(format!(
                "horsey input surface does not support movement axis {axis:?}"
            )),
            Axis::MouseX => modforge::input::l1::move_rel(delta, 0),
            Axis::MouseY => modforge::input::l1::move_rel(0, delta),
        }
    }
}

/// Register the Horsey input surface on the modforge global slot.
/// Called once from the worker thread at attach.
pub fn register() {
    modforge::input::set_input_surface(HorseyInputSurface::new());
    modforge::log!("horsey-mod: L3 input surface registered");
}
