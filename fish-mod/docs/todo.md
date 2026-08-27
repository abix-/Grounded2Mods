# fish-mod open issues

| Priority | System | Todo | Done when |
|---:|---|---|---|
| 1 | `fish-mod` | [x] Create the Cargo.toml as a cdylib depending on modforge and unityforge, and a minimal src/lib.rs that declares a ModDef with on_init registering built-in ops and selectors, wired with unityforge_mod! | `cargo check -p fish-mod` passes. |
| 2 | `cs-shim-mono` | [x] Install BepInEx 5 into the game directory and deploy the cs-shim-mono plugin so it loads the Rust cdylib at game startup | The game launches with BepInEx and the shim logs "Unityforge.Shim: ready" in the BepInEx log. |
| 3 | `fish-mod` | [x] Build the cdylib in release, copy it next to the shim as fish_mod.unityforge.dll, and confirm the HTTP control plane answers on the chosen port | `curl localhost:<port>/op` with a ping op returns a success response while the game runs. |
| 4 | `fish-mod` | [ ] Wire the test harness with a GameDef (AppID 4001890, process name "How to Fish.exe", HTTP probe on the chosen port) in tests/common/mod.rs | A test that calls harness.launch() and pings the control plane passes. |
| 5 | `fish-mod` | [ ] Run walk_class and list_singletons against the live game to map the game's type system and find the entry points for game state | A research doc listing the key classes and singletons exists in fish-mod/docs/research.md. |
