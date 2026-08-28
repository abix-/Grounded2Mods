//! In-game ImGui overlay via [hudhook](https://github.com/veeenu/hudhook).
//!
//! Generic across native-PE consumers. Any modforge consumer with the
//! `overlay-ui` Cargo feature enabled gets:
//!
//! - `arm(render_loop)`: install the DXGI Present + ResizeBuffers hooks,
//!   render `render_loop` in-game every frame.
//! - `disarm()`: best-effort unhook via hudhook's `eject` path.
//! - `is_armed()` / `frame_count()`: observability for HTTP ops or tests.
//!
//! Consumers implement [`ImguiRenderLoop`] (re-exported below) and call
//! [`arm`]. The simplest possible consumer:
//!
//! ```ignore
//! use modforge::ui::overlay::{self, ImguiRenderLoop, Ui, Condition};
//!
//! struct MyOverlay;
//! impl ImguiRenderLoop for MyOverlay {
//!     fn render(&mut self, ui: &mut Ui) {
//!         ui.window("My Mod")
//!             .size([400.0, 200.0], Condition::FirstUseEver)
//!             .build(|| ui.text("Hello, in-game!"));
//!     }
//! }
//!
//! overlay::arm(MyOverlay).expect("overlay arm failed");
//! ```
//!
//! ## Why not [`crate::ui::TabDef`]?
//!
//! `TabDef::render` is a bare `fn()` (no args) because the existing
//! ueforge / unityforge UE4SS bridge expects a thread-local ImGui
//! context. imgui-rs gives each render call a `&mut imgui::Ui` token,
//! which is a different signature. We expose hudhook's trait directly
//! rather than wrapping it; a tab-bar helper can land later if multiple
//! consumers want one.
//!
//! ## Why retire `native-ui` for overlay consumers?
//!
//! `native-ui` compiles modforge's vendored C++ ImGui shim. `overlay-ui`
//! pulls in imgui-sys, which bundles its own copy of the same C++. Two
//! static copies of ImGui in one cdylib produce LNK2005 multiply-defined
//! symbols at link time. A given consumer picks one feature or the
//! other.
//!
//! See [`docs/IN-GAME-OVERLAY-PLAN.md`](../../../docs/IN-GAME-OVERLAY-PLAN.md).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use hudhook::Hudhook;
use hudhook::hooks::dx11::ImguiDx11Hooks;

// Re-exports so consumers don't need to depend on hudhook + imgui
// directly. Anything they need to write a render loop is available
// from this one module.
pub use hudhook::ImguiRenderLoop;
pub use imgui::{Condition, Ui};

static ARMED: AtomicBool = AtomicBool::new(false);
static FRAMES: AtomicU64 = AtomicU64::new(0);

/// Wraps the consumer's render loop so we can count frames
/// transparently without burdening every consumer with the same
/// boilerplate.
struct FrameCountingLoop<R: ImguiRenderLoop> {
    inner: R,
}

impl<R: ImguiRenderLoop + Send + Sync + 'static> ImguiRenderLoop for FrameCountingLoop<R> {
    fn render(&mut self, ui: &mut Ui) {
        FRAMES.fetch_add(1, Ordering::Relaxed);
        self.inner.render(ui);
    }
}

/// Install the DXGI Present + ResizeBuffers hooks and start rendering
/// `render_loop` in-game. Idempotent: a second call while already armed
/// returns Ok and does not re-install.
///
/// The render loop runs on the game's render thread (whichever thread
/// calls `IDXGISwapChain::Present`). All `imgui-rs` calls inside
/// `render` must stay on that thread; do not pass the `&mut Ui` token
/// to other threads or store it.
pub fn arm<R: ImguiRenderLoop + Send + Sync + 'static>(render_loop: R) -> Result<(), String> {
    if ARMED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let wrapped = FrameCountingLoop { inner: render_loop };
    let hudhook = Hudhook::builder().with::<ImguiDx11Hooks>(wrapped).build();
    hudhook.apply().map_err(|status| {
        ARMED.store(false, Ordering::SeqCst);
        format!("hudhook apply failed: {status:?}")
    })
}

/// Best-effort unhook. Calls hudhook's `eject` which walks every
/// registered hook and removes it. Safe to call when not armed.
///
/// Hudhook's eject path is the same one its `inject` feature uses
/// for graceful unload; most overlay mods (including hudhook's own
/// examples) leave the hooks armed until DLL unload, which works too.
pub fn disarm() {
    if !ARMED.swap(false, Ordering::SeqCst) {
        return;
    }
    hudhook::eject();
}

/// True if `arm` succeeded and `disarm` has not been called since.
pub fn is_armed() -> bool {
    ARMED.load(Ordering::Relaxed)
}

/// Total frames rendered since the most recent successful `arm`. Wraps
/// at u64::MAX (effectively never on real hardware).
pub fn frame_count() -> u64 {
    FRAMES.load(Ordering::Relaxed)
}
