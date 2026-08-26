# scrapmechanic-mod

Better Survival custom game for Scrap Mechanic. Lua + JSON
content loaded by the game's built-in mod system.

## Game

- **Game:** Scrap Mechanic
- **Engine:** custom (C++ core, Lua 5.1 scripting, JSON data)
- **Mod system:** built-in (workshop or local `Survival/` override)
- **Language:** Lua, JSON

## Features

| Feature | Rating |
|---|---:|
| 1000-slot player inventory | 2/10 |
| No inventory drop on death | 2/10 |
| Three-times overworld unit spawn chances | 2/10 |
| Custom survival world and terrain bootstrap | 2/10 |
| Half fuel consumption (claimed by metadata, implementation not found) | 0/10 |
| No building restrictions (claimed by metadata, current script keeps restrictions enabled) | 0/10 |
| [Modding research](docs/research.md) (Lua, JSON, and native options) | 1/10 |

## Build

There is no compilation step. `BetterSurvival/` is the loadable
custom-game content directory.

## File layout

```
scrapmechanic-mod/
  BetterSurvival/
    description.json
    config.json
    Scripts/
    Terrain/
  README.md
  docs/
    research.md
```
