# quasimorph-mod

Quasimorph (.NET Framework 4.8) mod. C# project, not Rust.

## Game

- **Game:** Quasimorph
- **Engine:** Unity (Mono, .NET 4.8)
- **Language:** C#

## Features

| Feature | Rating |
|---|---:|
| [Mod framework](docs/research.md) (empty first-party API hook scaffold) | 1/10 |

## Build

```powershell
dotnet build quasimorph-mod\QuasimorphMod.csproj `
  -c Release `
  -p:GameDir="<game-root>"
```

Output: `quasimorph-mod/bin/Release/QuasimorphMod.dll`

## Deploy

```powershell
.\quasimorph-mod\scripts\build_and_deploy.ps1 -GameDir "<game-root>"
```

The game also requires `modmanifest.json`, generated from its
`mod_createmanifest` console command.

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
