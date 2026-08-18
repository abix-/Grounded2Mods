# quasimorph-mod

Quasimorph (.NET Framework 4.8) mod. C# project, not Rust.

## Game

- **Game:** Quasimorph
- **Engine:** Unity (Mono, .NET 4.8)
- **Language:** C#

## Features

| Feature | Rating |
|---|---:|
| [Mod framework](docs/research.md) (C# hooks into first-party mod API) | 1/10 |

## Build

```powershell
.\quasimorph-mod\scripts\build_and_deploy.ps1
```

Or manually:

```sh
dotnet build quasimorph-mod/QuasimorphMod.csproj -c Release
```

## File layout

```
quasimorph-mod/
  QuasimorphMod.csproj
  README.md
  docs/
  scripts/
    build_and_deploy.ps1
  src/
```
