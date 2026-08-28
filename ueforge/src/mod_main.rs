//! Mod metadata + entry-point wiring.
//!
//! Game crates declare a single `static MOD_INFO: ModDef = ...`
//! and call [`ue4ss_mod!`] once at the crate root. The macro emits
//! every `extern "C"` hook the shipped `ueforge_shim.cpp` calls
//! (`ueforge_mod_get_name`, `ueforge_mod_get_tab_count`,
//! `ueforge_mod_render_tab`, `ueforge_mod_dll_attach`,
//! `ueforge_mod_unreal_init`, `ueforge_mod_shutdown`), wires `DllMain`
//! into `ueforge::log::set_dll_module`, and initializes the logger
//! on `on_unreal_init`.
//!
//! Game side, in full:
//!
//! ```ignore
//! static MOD_INFO: ueforge::ModDef = ueforge::ModDef {
//!     name: "Outworld Station Mod",
//!     version: "0.1.0",
//!     log_file: "outworld_station_mod.log",
//!     console_title: "Outworld Station Mod",
//!     console: cfg!(feature = "console"),
//!     on_unreal_init: || { ows::start(); },
//!     on_shutdown: || { ows::stop(); },
//!     tabs: &[
//!         ueforge::TabDef { name: "Tweaks", render: ows::render_tweaks },
//!         ueforge::TabDef { name: "Debug",  render: ows::render_debug },
//!     ],
//! };
//!
//! ueforge::ue4ss_mod!(MOD_INFO);
//! ```
//!
//! No `cpp/`, no `build.rs` C++ wiring, no DllMain. Ueforge ships
//! all of it.

/// Root Def for an entire ueforge-based mod. The workspace's
/// canonical `<Subject>Def` for the mod itself. The shipped C++
/// shim reads this through the macro-emitted `extern "C"` hooks.
///
/// **NOT `#[non_exhaustive]`**: every consumer constructs `ModDef`
/// as a struct literal in their `lib.rs`. Adding a field IS a
/// breaking change for all consumers, but in this monorepo we
/// update them atomically. Document new fields in `changelog.md`.
pub struct ModDef {
    /// Display name (UTF-8). Becomes UE4SS `ModName`.
    pub name: &'static str,
    pub version: &'static str,
    /// File written to under `<DLL_dir>/<log_file>`. Pass an empty
    /// string to disable file logging.
    pub log_file: &'static str,
    pub console_title: &'static str,
    /// Whether to pop a console window via `AllocConsole` on init.
    /// Typically `cfg!(feature = "console")`.
    pub console: bool,
    /// Called once on UE4SS `on_unreal_init`. Heavy lifting goes
    /// here (PE hooks, debug server, worker thread spawn).
    pub on_unreal_init: fn(),
    /// Called once when UE4SS destructs the mod.
    pub on_shutdown: fn(),
    /// Tabs to register in UE4SS's debug overlay. Order is stable
    ///. The shipped shim trampolines dispatch by index. Bare
    /// slice (documented carve-out from the `<Subject>Registry`
    /// wrapper rule. There are no extra registry-level fields
    /// beyond "these are the tabs", and a wrapper would be pure
    /// ceremony).
    pub tabs: &'static [TabDef],
}

// `TabDef` lifted to `modforge::ui::TabDef` during Phase 0b
// row 21. Re-exported here so existing ueforge call sites
// compile unchanged.
pub use modforge::ui::TabDef;

/// Wire a `static ModDef` into the shipped C++ shim. Emits the
/// full set of `extern "C"` hooks plus `DllMain`.
///
/// Call once at the crate root.
#[macro_export]
macro_rules! ue4ss_mod {
    ($mod_info:ident) => {
        const _: &$crate::mod_main::ModDef = &$mod_info;

        #[unsafe(no_mangle)]
        pub extern "C" fn ueforge_mod_get_name(out_len: *mut usize) -> *const u8 {
            unsafe {
                *out_len = $mod_info.name.len();
            }
            $mod_info.name.as_ptr()
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ueforge_mod_get_version(out_len: *mut usize) -> *const u8 {
            unsafe {
                *out_len = $mod_info.version.len();
            }
            $mod_info.version.as_ptr()
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ueforge_mod_get_log_file(out_len: *mut usize) -> *const u8 {
            unsafe {
                *out_len = $mod_info.log_file.len();
            }
            $mod_info.log_file.as_ptr()
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ueforge_mod_get_console_title(out_len: *mut usize) -> *const u8 {
            unsafe {
                *out_len = $mod_info.console_title.len();
            }
            $mod_info.console_title.as_ptr()
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ueforge_mod_get_tab_count() -> u32 {
            $mod_info.tabs.len() as u32
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ueforge_mod_get_tab_name(idx: u32, out_len: *mut usize) -> *const u8 {
            let i = idx as usize;
            if i >= $mod_info.tabs.len() {
                unsafe {
                    *out_len = 0;
                }
                return ::std::ptr::null();
            }
            let s = $mod_info.tabs[i].name;
            unsafe {
                *out_len = s.len();
            }
            s.as_ptr()
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ueforge_mod_dll_attach(hmod: *mut ::std::ffi::c_void) {
            // Forward HMODULE into the logger's DLL-dir resolver.
            $crate::log::set_dll_module(hmod as _);
        }

        /// UE4SS's `CppUserModBase::on_update`, once per frame
        /// for the life of the process: main menu, loading
        /// screens, and play alike.
        #[unsafe(no_mangle)]
        pub extern "C" fn ueforge_mod_update() {
            $crate::frame::run_update();
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ueforge_mod_unreal_init() {
            // Initialize the logger on first unreal_init so its
            // open call lands on the same thread that prints the
            // first line.
            if !$mod_info.log_file.is_empty() {
                $crate::log::init($crate::log::Config {
                    file_name: $mod_info.log_file,
                    console_title: $mod_info.console_title,
                    console: $mod_info.console,
                });
            }
            // Clean up the old image's file from the previous
            // hot-reload generation, if any. Best-effort; failure
            // is logged + ignored.
            $crate::mod_main::cleanup_old_dll();
            // Auto-apply any pending main-new.dll dropped by
            // `cargo deploy install` while the game was closed.
            // The CURRENT process keeps running its already-loaded
            // image; the swap makes the next reload (Ctrl+R or
            // relaunch) pick up the new code without forcing the
            // user to manually rename files.
            $crate::mod_main::apply_pending_swap_at_init();
            // Background watcher: polls for `main-new.dll`
            // appearing post-launch (deploy while game is open)
            // and synthesizes Ctrl+R to trigger UE4SS's reload
            // path. Zero manual steps from `cargo deploy install`
            // to new code running. As long as the game has
            // focus.
            $crate::hot_reload::spawn_watcher();
            ($mod_info.on_unreal_init)();
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ueforge_mod_shutdown() {
            // 1. Game's on_shutdown. Still kept as a special
            //    pre-registry hook so the game can do mod-
            //    specific teardown that's awkward to express as
            //    a `fn()` (closures over module statics, etc.).
            //    Game crates that fit the registry pattern can
            //    leave on_shutdown empty and register their
            //    handlers via `ueforge::shutdown::SHUTDOWN_REGISTRY`.
            ($mod_info.on_shutdown)();
            // 2. Framework + game shutdown handlers, in order.
            //    Built-ins (hooks=100, http=200, settings=300,
            //    freeze sweeper=400) are registered immediately
            //    below; game crates can interleave their own at
            //    any order via `SHUTDOWN_REGISTRY.register`.
            $crate::shutdown::register_builtins();
            $crate::shutdown::SHUTDOWN_REGISTRY.run_all();
            // 3. Side-file swap so UE4SS's next LoadLibraryExW
            //    picks up the new image written to main-new.dll.
            $crate::mod_main::finalize_hot_reload_swap();
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn ueforge_mod_render_tab(idx: u32) {
            // Bridge our locally-linked imgui at UE4SS's context.
            // The extern call is declared at the macro use site
            // (the game crate's cdylib) so ueforge.rlib itself never
            // references ueforge_ui_enable_imgui. Letting build
            // scripts and test binaries that depend on ueforge
            // link cleanly without ueforge_shim.cpp's UE4SS deps.
            unsafe extern "C" {
                fn ueforge_ui_enable_imgui();
            }
            unsafe { ueforge_ui_enable_imgui() };
            let i = idx as usize;
            if let Some(t) = $mod_info.tabs.get(i) {
                (t.render)();
            }
        }
    };
}

/// Hot-reload swap. Called by the macro's `ueforge_mod_shutdown`
/// after the game's on_shutdown returns. If `cargo deploy install`
/// wrote a `main-new.dll` next to the running `main.dll`, swap
/// them so UE4SS's next `LoadLibraryExW` picks up the new image.
///
/// Filesystem ops:
/// 1. Rename `main.dll` -> `main-old.dll` (allowed because
///    `LoadLibraryExW` opens with `FILE_SHARE_DELETE`).
/// 2. Rename `main-new.dll` -> `main.dll` (vacant target).
///
/// Failures are logged + ignored. The worst case is UE4SS
/// reloads the same image, the user notices "no new code" and
/// retries deploy. Better than killing the game.
///
/// See `ueforge/docs/lifecycle.md` "side-file pattern" for the
/// full sequence + the empirical verification.
#[doc(hidden)]
pub fn finalize_hot_reload_swap() {
    let Some(dir) = crate::log::dll_dir() else {
        crate::log!("ueforge: hot-reload swap skipped (dll_dir unresolved)");
        return;
    };
    let new_dll = dir.join("main-new.dll");
    if !new_dll.is_file() {
        // No pending update. Normal Ctrl+R or process exit.
        return;
    }
    let main_dll = dir.join("main.dll");
    let old_dll = dir.join("main-old.dll");

    // Best-effort cleanup of any prior generation's leftover.
    let _ = std::fs::remove_file(&old_dll);

    if let Err(e) = std::fs::rename(&main_dll, &old_dll) {
        crate::log!(
            "ueforge: hot-reload swap step 1 failed (rename main.dll -> main-old.dll): {e}"
        );
        return;
    }
    if let Err(e) = std::fs::rename(&new_dll, &main_dll) {
        crate::log!(
            "ueforge: hot-reload swap step 2 failed (rename main-new.dll -> main.dll): {e}; rolling back"
        );
        // Roll back step 1 so we leave the dir consistent.
        let _ = std::fs::rename(&old_dll, &main_dll);
        return;
    }
    crate::log!(
        "ueforge: hot-reload swap complete (main-new.dll -> main.dll); UE4SS will load the new image"
    );
}

/// Init-time auto-swap. If `cargo deploy install` was run while
/// the game was closed (or while the previous mod generation
/// didn't get a clean shutdown), `main-new.dll` will be sitting
/// next to `main.dll`. We can't reload ourselves mid-init. The
/// current process keeps using the already-loaded image. But
/// we CAN rename the files now so that the next UE4SS reload
/// (Ctrl+R or next launch) picks up the new code.
///
/// The current image continues to run from its (now-renamed)
/// main-old.dll mapping. Windows' `FILE_SHARE_DELETE` allows
/// the in-use file to be renamed.
#[doc(hidden)]
pub fn apply_pending_swap_at_init() {
    let Some(dir) = crate::log::dll_dir() else {
        return;
    };
    let new_dll = dir.join("main-new.dll");
    if !new_dll.is_file() {
        return;
    }
    let main_dll = dir.join("main.dll");
    let old_dll = dir.join("main-old.dll");

    // Best-effort cleanup of any prior generation's leftover so
    // step-1's rename target is vacant.
    let _ = std::fs::remove_file(&old_dll);

    if let Err(e) = std::fs::rename(&main_dll, &old_dll) {
        crate::log!("ueforge: init-time swap step 1 failed (rename main.dll -> main-old.dll): {e}");
        return;
    }
    if let Err(e) = std::fs::rename(&new_dll, &main_dll) {
        crate::log!(
            "ueforge: init-time swap step 2 failed (rename main-new.dll -> main.dll): {e}; rolling back"
        );
        let _ = std::fs::rename(&old_dll, &main_dll);
        return;
    }
    crate::log!(
        "ueforge: init-time swap applied (main-new.dll -> main.dll); press Ctrl+R or relaunch to load the new image"
    );
}

/// Best-effort cleanup of a stale `main-old.dll` left over from a
/// previous hot-reload generation. Called once during
/// `ueforge_mod_unreal_init` so the new image (which IS the
/// freshly-renamed main.dll) clears the leftover sibling. The
/// previous image's `FreeLibrary` already happened by the time
/// our init runs, so the file is deletable.
#[doc(hidden)]
pub fn cleanup_old_dll() {
    let Some(dir) = crate::log::dll_dir() else {
        return;
    };
    let old_dll = dir.join("main-old.dll");
    if old_dll.is_file() {
        match std::fs::remove_file(&old_dll) {
            Ok(()) => crate::log!("ueforge: cleaned up main-old.dll from previous hot-reload"),
            Err(e) => {
                crate::log!("ueforge: could not remove main-old.dll: {e} (will retry next init)")
            }
        }
    }
}
