# Building

> **Authoritative on:** workspace build prerequisites.

## Prerequisites

- Windows 10/11 x64
- Rust toolchain through rustup, with stable pinned by
  [`rust-toolchain.toml`](../rust-toolchain.toml)
- Visual Studio Build Tools 2022 or newer with the C++ workload
- For ueforge mods, the target game's UE4SS installation
- For unityforge Mono mods, BepInEx
- For unityforge IL2CPP mods, MelonLoader
- For framework development, clone with `--recurse-submodules`.
  Dear ImGui v1.92.1 lives in a submodule.
