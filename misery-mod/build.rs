// Compile ueforge's default factory + ImGui context bridge into the
// cdylib. ueforge's own build.rs already compiled the imgui vendor
// + ueforge_ui bridge into the static lib that cargo auto-links.
fn main() {
    ueforge::build::CppShim::new().compile();
}
