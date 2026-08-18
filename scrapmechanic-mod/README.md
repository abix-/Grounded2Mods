# scrapmechanic-mod

Scrap Mechanic survival mod. Lua + JSON data mod using the
game's built-in mod system.

## Game

- **Game:** Scrap Mechanic
- **Engine:** custom (C++ core, Lua 5.1 scripting, JSON data)
- **Mod system:** built-in (workshop or local `Survival/` override)
- **Language:** Lua, JSON

## Features

| Feature | Rating |
|---|---:|
| [Survival overhaul](docs/research.md) (Lua + JSON data mod) | 1/10 |

## Structure

The `BetterSurvival/` directory is the mod content root. It
contains the standard Scrap Mechanic mod layout: `description.json`
for metadata, `Scripts/` for Lua, and data directories for game
object definitions.

## File layout

```
scrapmechanic-mod/
  README.md
  docs/
  BetterSurvival/
    description.json
    config.json
    Scripts/
    Objects/
    Characters/
    Tools/
    Harvestables/
    ...
```
