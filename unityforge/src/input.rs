//! Per-keypress callback binding.
//!
//! The active C# shim polls every registered binding on every
//! Update tick and fires the Rust callback synchronously on the
//! Unity main thread. Backend-agnostic: works on both Mono and
//! IL2CPP through the v3+ bridge.
//!
//! ```ignore
//! extern "C" fn on_space() {
//!     // runs on Unity main thread; safe to call bridge ops
//!     unityforge::mono::log(
//!         unityforge::mono::LogLevel::Info, "space pressed");
//! }
//!
//! unityforge::input::register_key_press(KeyCode::Space, on_space);
//! ```

use std::time::Duration;

use modforge::input::{Axis, Button, InputSurface, Key, PlayerCommand};
use modforge::route::PlayerObservation;

use crate::bridge;
use crate::main_thread_queue::{MAIN_QUEUE, MainThreadQueue};

const INPUT_TIMEOUT: Duration = Duration::from_secs(3);

type PlayerInput = dyn Fn(&[PlayerCommand]) -> Result<(), String> + Send + Sync;
type ObservePlayer = dyn Fn() -> Result<PlayerObservation, String> + Send + Sync;

pub struct UnityInputSurface {
    name: &'static str,
    queue: &'static MainThreadQueue,
    player_input: Box<PlayerInput>,
    observe_player: Box<ObservePlayer>,
}

impl UnityInputSurface {
    pub fn new(
        name: &'static str,
        queue: &'static MainThreadQueue,
        player_input: impl Fn(&[PlayerCommand]) -> Result<(), String> + Send + Sync + 'static,
        observe_player: impl Fn() -> Result<PlayerObservation, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name,
            queue,
            player_input: Box::new(player_input),
            observe_player: Box::new(observe_player),
        }
    }
}

impl InputSurface for &'static UnityInputSurface {
    fn name(&self) -> &'static str {
        self.name
    }

    fn click(&self, _button: Button, _x: i32, _y: i32) -> Result<(), String> {
        Err("absolute UI clicks are not implemented by Unity player input".into())
    }

    fn move_abs(&self, _x: i32, _y: i32) -> Result<(), String> {
        Err("absolute cursor movement is not implemented by Unity player input".into())
    }

    fn move_rel(&self, dx: i32, dy: i32) -> Result<(), String> {
        self.commands(&[PlayerCommand::mouse_delta(dx, dy)])
    }

    fn key(&self, key: Key, down: bool) -> Result<(), String> {
        self.commands(&[PlayerCommand::key(key, down)])
    }

    fn axis(&self, axis: Axis, value: f32, _delta_time: f32) -> Result<(), String> {
        match axis {
            Axis::MouseX => self.move_rel(value.round() as i32, 0),
            Axis::MouseY => self.move_rel(0, value.round() as i32),
            Axis::MoveForward | Axis::MoveRight => Err(format!(
                "Unity bot movement requires virtual keys, not axis {axis:?}"
            )),
        }
    }

    fn commands(&self, commands: &[PlayerCommand]) -> Result<(), String> {
        let surface = *self;
        let commands = commands.to_vec();
        self.queue
            .run_result("Unity player input", INPUT_TIMEOUT, move || {
                (surface.player_input)(&commands)
            })
    }

    fn observe_player(&self) -> Result<PlayerObservation, String> {
        let surface = *self;
        self.queue
            .run_result("Unity player observation", INPUT_TIMEOUT, move || {
                (surface.observe_player)()
            })
    }
}

pub fn register_player_input(
    name: &'static str,
    player_input: impl Fn(&[PlayerCommand]) -> Result<(), String> + Send + Sync + 'static,
    observe_player: impl Fn() -> Result<PlayerObservation, String> + Send + Sync + 'static,
) {
    let surface: &'static UnityInputSurface = Box::leak(Box::new(UnityInputSurface::new(
        name,
        &MAIN_QUEUE,
        player_input,
        observe_player,
    )));
    modforge::input::set_input_surface(surface);
}

/// Unity `KeyCode` integer values. Subset; add more as needed.
/// Full enum: https://docs.unity3d.com/ScriptReference/KeyCode.html
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Space = 32,
    Return = 13,
    Escape = 27,
    Tab = 9,
    LeftShift = 304,
    RightShift = 303,
    LeftControl = 306,
    RightControl = 305,
    LeftAlt = 308,
    RightAlt = 307,
    F1 = 282,
    F2 = 283,
    F3 = 284,
    F4 = 285,
    F5 = 286,
    F6 = 287,
    F7 = 288,
    F8 = 289,
    F9 = 290,
    F10 = 291,
    F11 = 292,
    F12 = 293,
    Insert = 277,
    Delete = 127,
    Home = 278,
    End = 279,
    PageUp = 280,
    PageDown = 281,
    UpArrow = 273,
    DownArrow = 274,
    RightArrow = 275,
    LeftArrow = 276,
    A = 97,
    B = 98,
    C = 99,
    D = 100,
    E = 101,
    F = 102,
    G = 103,
    H = 104,
    I = 105,
    J = 106,
    K = 107,
    L = 108,
    M = 109,
    N = 110,
    O = 111,
    P = 112,
    Q = 113,
    R = 114,
    S = 115,
    T = 116,
    U = 117,
    V = 118,
    W = 119,
    X = 120,
    Y = 121,
    Z = 122,
    Alpha0 = 48,
    Alpha1 = 49,
    Alpha2 = 50,
    Alpha3 = 51,
    Alpha4 = 52,
    Alpha5 = 53,
    Alpha6 = 54,
    Alpha7 = 55,
    Alpha8 = 56,
    Alpha9 = 57,
}

/// Register a callback that fires on every fresh keypress (one
/// fire per press, same semantics as `Input.GetKeyDown`).
/// Returns a binding handle that can be passed to
/// [`unregister`]. Returns `None` if the bridge isn't
/// installed yet.
pub fn register_key_press(key: KeyCode, callback: extern "C" fn()) -> Option<i32> {
    let bridge = bridge::get()?;
    let handle = (bridge.register_key_binding)(key as i32, callback);
    if handle == 0 { None } else { Some(handle) }
}

/// Drop a key binding. Idempotent.
pub fn unregister(binding: i32) {
    let Some(bridge) = bridge::get() else { return };
    (bridge.unregister_key_binding)(binding);
}
