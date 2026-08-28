// Compile ueforge's bundled C++ that EVERY mod always needs:
// imgui v1.92.1 vendor + ueforge_ui.cpp (the ImGui Rust bridge) +
// the ImGui-context bridge to UE4SS.dll. Cargo links the
// resulting static lib into every consumer of this crate.
//
// ueforge_shim.cpp (the default UE4SS factory: start_mod /
// uninstall_mod / DllMain plus the UespyMod CppUserModBase
// subclass) is intentionally NOT compiled here. Mods that want
// the default factory call `ueforge::build::CppShim::new()
// .compile()` from their build.rs; mods with their own shim
// (grounded2-mod today) skip the default. Otherwise factory
// symbols would collide.
//
// /DELAYLOAD:UE4SS.dll keeps build-script binaries linkable. The
// only function in ueforge_ui.cpp that touches UE4SS is
// `ueforge_ui_enable_imgui`; it's never called from a build script
// or test binary, so delay-loaded symbols never resolve there.

fn main() {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cpp = manifest.join("cpp");
    let imgui = cpp.join("imgui");
    let ue4ss = manifest.join("ue4ss");

    // The imgui directory is a git submodule pinned to v1.92.1
    // (matches UE4SS's bundled version). If it's empty, the user
    // cloned without `--recurse-submodules`. Fail loudly with a fix.
    if !imgui.join("imgui.cpp").exists() {
        panic!(
            "ueforge: cpp/imgui submodule is not initialized.\n\
             Run from the repo root:\n\
                 git submodule update --init --recursive\n\
             (or re-clone with `git clone --recurse-submodules <url>`)."
        );
    }

    println!("cargo:rerun-if-changed={}", cpp.display());
    println!(
        "cargo:rerun-if-changed={}",
        ue4ss.join("UE4SS.lib").display()
    );

    // UE4SS.lib symbol-presence check. The two C++ exports
    // ueforge_shim.cpp imports from UE4SS.lib are the bridge
    // surface to UE4SS's ImGui context. If UE4SS upstream renames
    // / removes either of them the link error happens at the
    // CONSUMER cdylib's build (via ueforge::build::CppShim::compile),
    // sometimes far from the actual symbol drift. Catch the
    // regression here at the framework build instead.
    //
    // A .lib is an ar-style archive; mangled symbol names appear
    // as literal substrings in the export records. A naive byte
    // scan is sufficient + avoids parsing the COFF format. The
    // two MSVC-mangled names below are unique enough that a
    // collision with unrelated bytes is implausible.
    let ue4ss_lib = ue4ss.join("UE4SS.lib");
    if ue4ss_lib.exists() {
        const REQUIRED_SYMBOLS: &[&str] = &[
            "?get_current_imgui_context@UE4SSProgram@RC@@",
            "?get_current_imgui_allocator_functions@UE4SSProgram@RC@@",
        ];
        match std::fs::read(&ue4ss_lib) {
            Ok(bytes) => {
                let mut missing: Vec<&str> = Vec::new();
                for sym in REQUIRED_SYMBOLS {
                    let needle = sym.as_bytes();
                    let found = bytes.windows(needle.len()).any(|w| w == needle);
                    if !found {
                        missing.push(sym);
                    }
                }
                if !missing.is_empty() {
                    panic!(
                        "ueforge: UE4SS.lib at {} is missing the symbols\
                         \n  {}\
                         \nueforge_shim.cpp will not link.\
                         \nThe file is probably from an incompatible UE4SS\
                         \nversion. Regenerate it from your installed\
                         \nUE4SS.dll's exports (see ueforge/ue4ss/README\
                         \nor the script that originally produced the .lib).",
                        ue4ss_lib.display(),
                        missing.join("\n  ")
                    );
                }
            }
            Err(e) => {
                println!(
                    "cargo:warning=ueforge: could not read {} for symbol \
                     check ({e}); proceeding without verification",
                    ue4ss_lib.display()
                );
            }
        }
    } else {
        println!(
            "cargo:warning=ueforge: {} not found; skipping symbol-presence \
             check. Build will fail at link time if a consumer cdylib pulls \
             in ueforge_shim.cpp.",
            ue4ss_lib.display()
        );
    }

    cc::Build::new()
        .cpp(true)
        .std("c++20")
        .include(&imgui)
        .include(&cpp)
        .file(imgui.join("imgui.cpp"))
        .file(imgui.join("imgui_draw.cpp"))
        .file(imgui.join("imgui_tables.cpp"))
        .file(imgui.join("imgui_widgets.cpp"))
        .file(cpp.join("ueforge_ui.cpp"))
        .file(cpp.join("ueforge_seh.cpp"))
        .flag_if_supported("/EHsc")
        .static_crt(false)
        .warnings(false)
        .compile("ueforge_native");

    // No UE4SS.lib link here. ueforge_ui.cpp has no UE4SS deps.
    // the context bridge lives in ueforge_shim.cpp, which only the
    // game's cdylib link pulls in (via ueforge::build::CppShim).
    let _ = ue4ss; // path silenced; CppShim emits the link.
}
