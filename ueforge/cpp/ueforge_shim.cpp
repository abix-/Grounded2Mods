// ueforge_shim.cpp
//
// Generic UE4SS mod shim shipped in ueforge. Compiled by ueforge-build
// into every UE4SS Rust mod's cdylib. The mod's metadata + tabs +
// lifecycle hooks all come from extern "C" Rust functions that the
// `ueforge::ue4ss_mod!` macro emits. The mod owns no C++ source.
//
// Bridges:
//   ueforge_mod_get_name()          -> *const u8 / size  (UTF-8)
//   ueforge_mod_get_version()       -> *const u8 / size
//   ueforge_mod_get_log_file()      -> *const u8 / size
//   ueforge_mod_get_console_title() -> *const u8 / size
//   ueforge_mod_get_tab_count()     -> u32
//   ueforge_mod_get_tab_name(idx)   -> *const u8 / size
//   ueforge_mod_dll_attach(hmod)    -> void
//   ueforge_mod_unreal_init()       -> void
//   ueforge_mod_shutdown()          -> void
//   ueforge_mod_render_tab(idx)     -> void
//
// All extern "C" Rust functions are emitted by the macro from a
// single `static MOD_INFO: ModInfo = ...` declaration in the
// game's lib.rs.

#include <Windows.h>
#include <array>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#include "imgui.h"
#include "ueforge_cppusermodbase.hpp"
#include "ueforge_imgui_bridge.hpp"

// ---- ImGui context bridge ----
//
// Called once per tab-render callback to point our locally-linked
// ImGui at UE4SS's context + matching allocator. Lives in this
// file (not ueforge_ui.cpp) so the UE4SS.lib dependency is opt-in:
// only mods that compile ueforge_shim.cpp (via ueforge::build::CppShim)
// pull in UE4SS.dll. Test and build-script binaries skip both.

extern "C" void ueforge_ui_enable_imgui() {
    ImGui::SetCurrentContext(RC::UE4SSProgram::get_current_imgui_context());
    ImGuiMemAllocFunc alloc_func{};
    ImGuiMemFreeFunc  free_func{};
    void* user_data{};
    RC::UE4SSProgram::get_current_imgui_allocator_functions(
        &alloc_func, &free_func, &user_data);
    ImGui::SetAllocatorFunctions(alloc_func, free_func, user_data);
}

// ---- extern "C" Rust hooks ----

extern "C" {
    const std::uint8_t* ueforge_mod_get_name(std::size_t* out_len);
    const std::uint8_t* ueforge_mod_get_version(std::size_t* out_len);
    const std::uint8_t* ueforge_mod_get_log_file(std::size_t* out_len);
    const std::uint8_t* ueforge_mod_get_console_title(std::size_t* out_len);
    std::uint32_t       ueforge_mod_get_tab_count();
    const std::uint8_t* ueforge_mod_get_tab_name(std::uint32_t idx, std::size_t* out_len);
    void                ueforge_mod_dll_attach(void* hmod);
    void                ueforge_mod_unreal_init();
    void                ueforge_mod_update();
    void                ueforge_mod_shutdown();
    void                ueforge_mod_render_tab(std::uint32_t idx);
}

// ---- string helpers ----

namespace {
RC::StringType utf8_to_wstring(const std::uint8_t* bytes, std::size_t len) {
    int n = MultiByteToWideChar(CP_UTF8, 0,
        reinterpret_cast<const char*>(bytes), static_cast<int>(len),
        nullptr, 0);
    RC::StringType out(static_cast<std::size_t>(n), L'\0');
    MultiByteToWideChar(CP_UTF8, 0,
        reinterpret_cast<const char*>(bytes), static_cast<int>(len),
        out.data(), n);
    return out;
}

RC::StringType call_str_hook(const std::uint8_t* (*hook)(std::size_t*)) {
    std::size_t n = 0;
    const std::uint8_t* p = hook(&n);
    if (!p || n == 0) return RC::StringType{};
    return utf8_to_wstring(p, n);
}
} // namespace

// ---- mod class ----

namespace {

// Per-tab dispatcher state. `register_tab` takes a static
// `RenderFunctionType = void (*)(CppUserModBase*)`, but we need to
// pass an index alongside it. Workaround: emit one trampoline per
// tab index (up to MAX_TABS), each calling
// `ueforge_mod_render_tab(idx)`. MAX_TABS chosen generously.

constexpr std::uint32_t MAX_TABS = 16;

template <std::uint32_t Idx>
void render_tab_trampoline(RC::CppUserModBase*) {
    ueforge_mod_render_tab(Idx);
}

using TrampolineFn = void (*)(RC::CppUserModBase*);

template <std::uint32_t... Is>
constexpr auto build_trampoline_table(std::integer_sequence<std::uint32_t, Is...>) {
    return std::array<TrampolineFn, sizeof...(Is)>{ &render_tab_trampoline<Is>... };
}

template <std::uint32_t N>
constexpr auto trampoline_table() {
    return build_trampoline_table(std::make_integer_sequence<std::uint32_t, N>{});
}

const auto& tab_trampolines() {
    static auto table = trampoline_table<MAX_TABS>();
    return table;
}

class UespyMod : public RC::CppUserModBase {
  public:
    UespyMod() : CppUserModBase() {
        ModName    = call_str_hook(ueforge_mod_get_name);
        ModVersion = call_str_hook(ueforge_mod_get_version);

        std::uint32_t count = ueforge_mod_get_tab_count();
        if (count > MAX_TABS) count = MAX_TABS;
        for (std::uint32_t i = 0; i < count; ++i) {
            std::size_t n = 0;
            const std::uint8_t* p = ueforge_mod_get_tab_name(i, &n);
            if (!p || n == 0) continue;
            register_tab(utf8_to_wstring(p, n), tab_trampolines()[i]);
        }
    }

    ~UespyMod() override {
        ueforge_mod_shutdown();
    }

    auto on_unreal_init() -> void override {
        ueforge_mod_unreal_init();
    }

    // Called by UE4SS every frame, regardless of whether a world
    // is loaded. This is how the mod reaches the main menu.
    auto on_update() -> void override {
        ueforge_mod_update();
    }
};

} // namespace

// ---- DllMain + factory exports ----

extern "C" __declspec(dllexport) auto start_mod() -> RC::CppUserModBase* {
    return new UespyMod();
}

extern "C" __declspec(dllexport) auto uninstall_mod(RC::CppUserModBase* mod) -> void {
    delete mod;
}

BOOL APIENTRY DllMain(HMODULE hmod, DWORD reason, LPVOID /*reserved*/) {
    if (reason == DLL_PROCESS_ATTACH) {
        ueforge_mod_dll_attach(hmod);
    }
    return TRUE;
}
