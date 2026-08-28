//! Build-time helper for game `build.rs` scripts.
//!
//! Compiles a mod's C++ shim (default `ueforge_shim.cpp` shipped by
//! ueforge, plus any game-specific source) on top of the always-on
//! C++ that ueforge's own `build.rs` already compiled
//! (`ueforge_ui.cpp` + imgui + UE4SS.lib link).
//!
//! ```ignore
//! // new mod (default shim, no custom C++):
//! fn main() {
//!     ueforge::build::CppShim::new().compile();
//! }
//!
//! // mod with its own shim:
//! fn main() {
//!     ueforge::build::CppShim::new()
//!         .source("cpp/shim.cpp")
//!         .skip_default_shim()
//!         .compile();
//! }
//! ```

use std::path::{Path, PathBuf};

const UESPY_CPPUSERMODBASE_HPP: &str = include_str!("../cpp/ueforge_cppusermodbase.hpp");
const UESPY_IMGUI_BRIDGE_HPP: &str = include_str!("../cpp/ueforge_imgui_bridge.hpp");
const UESPY_SHIM_CPP: &str = include_str!("../cpp/ueforge_shim.cpp");

#[derive(Default)]
pub struct CppShim {
    source: Option<PathBuf>,
    skip_default_shim: bool,
    extra_includes: Vec<PathBuf>,
    extra_sources: Vec<PathBuf>,
}

impl CppShim {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a game-specific shim or other C++ TU. Use when the mod
    /// needs custom C++ alongside (or instead of) the shipped
    /// `ueforge_shim.cpp`.
    pub fn source<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.source = Some(path.into());
        self
    }

    /// Don't compile the shipped `ueforge_shim.cpp`. Required when
    /// the mod ships its own factory exports (`start_mod`,
    /// `uninstall_mod`, `DllMain`); otherwise the linker fails on
    /// duplicate symbols.
    pub fn skip_default_shim(mut self) -> Self {
        self.skip_default_shim = true;
        self
    }

    /// Add a directory to the C++ include path.
    pub fn extra_include<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.extra_includes.push(path.into());
        self
    }

    /// Add an extra C++ source file.
    pub fn extra_source<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.extra_sources.push(path.into());
        self
    }

    pub fn compile(self) {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR not set (run from a build script)");
        let manifest_dir = PathBuf::from(manifest_dir);
        let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
        let out_dir = PathBuf::from(out_dir);

        // Drop shared ueforge headers into OUT_DIR/ueforge_cpp/ so a
        // game's shim can `#include "ueforge_cppusermodbase.hpp"`
        // without managing a relative path to ueforge/cpp/.
        let ueforge_cpp = out_dir.join("ueforge_cpp");
        std::fs::create_dir_all(&ueforge_cpp)
            .unwrap_or_else(|e| panic!("ueforge::build: create {:?}: {e}", ueforge_cpp));
        write_if_changed(
            &ueforge_cpp.join("ueforge_cppusermodbase.hpp"),
            UESPY_CPPUSERMODBASE_HPP,
        );
        write_if_changed(
            &ueforge_cpp.join("ueforge_imgui_bridge.hpp"),
            UESPY_IMGUI_BRIDGE_HPP,
        );

        let mut sources_to_compile: Vec<PathBuf> = Vec::new();

        if !self.skip_default_shim {
            let shim_dst = ueforge_cpp.join("ueforge_shim.cpp");
            write_if_changed(&shim_dst, UESPY_SHIM_CPP);
            sources_to_compile.push(shim_dst);
        }

        if let Some(p) = self.source {
            let abs = absolutize(&manifest_dir, p);
            println!("cargo:rerun-if-changed={}", abs.display());
            sources_to_compile.push(abs);
        }
        for src in &self.extra_sources {
            let abs = absolutize(&manifest_dir, src.clone());
            println!("cargo:rerun-if-changed={}", abs.display());
            sources_to_compile.push(abs);
        }

        if sources_to_compile.is_empty() {
            // Nothing to compile means: no factory shim AND no
            // game source. The mod can't load via UE4SS without
            // factory exports. Flag it loudly.
            panic!(
                "ueforge::build: nothing to compile. Either keep the \
                 default shim (don't call skip_default_shim) or \
                 provide a custom source(...)"
            );
        }

        // imgui vendor is already compiled into ueforge_native.lib
        // by ueforge's own build.rs. Just include its headers so
        // shim.cpp can `#include "imgui.h"`.
        let imgui = ueforge_root().join("cpp").join("imgui");

        let mut build = cc::Build::new();
        build
            .cpp(true)
            .std("c++20")
            .include(&ueforge_cpp)
            .include(&imgui)
            .flag_if_supported("/EHsc")
            .static_crt(false)
            .warnings(false);
        for src in &sources_to_compile {
            build.file(src);
        }
        for inc in &self.extra_includes {
            build.include(absolutize(&manifest_dir, inc.clone()));
        }

        let objs = build.compile_intermediates();
        for obj in &objs {
            println!("cargo:rustc-cdylib-link-arg={}", obj.display());
        }

        // UE4SS.lib only the cdylib needs; emit per-cdylib link
        // args so test / build-script binaries don't depend on
        // UE4SS.dll being on PATH.
        let ue4ss = ueforge_root().join("ue4ss");
        println!(
            "cargo:rerun-if-changed={}",
            ue4ss.join("UE4SS.lib").display()
        );
        println!("cargo:rustc-cdylib-link-arg=/LIBPATH:{}", ue4ss.display());
        println!("cargo:rustc-cdylib-link-arg=UE4SS.lib");
    }
}

fn ueforge_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn absolutize(manifest_dir: &Path, p: PathBuf) -> PathBuf {
    if p.is_absolute() {
        p
    } else {
        manifest_dir.join(p)
    }
}

fn write_if_changed(path: &Path, contents: &str) {
    let existing = std::fs::read_to_string(path).ok();
    if existing.as_deref() != Some(contents) {
        std::fs::write(path, contents)
            .unwrap_or_else(|e| panic!("ueforge::build: write {:?}: {e}", path));
    }
}
