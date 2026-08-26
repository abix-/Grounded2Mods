# scrapmechanic-mod

Better Survival custom game for Scrap Mechanic. Lua + JSON
content loaded by the game's built-in mod system.

## Game

- **Game:** Scrap Mechanic
- **Engine:** custom (C++ core, Lua 5.1 scripting, JSON data)
- **Mod system:** built-in (workshop or local `Survival/` override)
- **Language:** Lua, JSON

## Features

| Feature | Description | Rating |
|---|---|---:|
| [Inventory](BetterSurvival/Scripts/Game.lua) | Sets the player inventory to 1000 slots. | 2/10 |
| [Death](BetterSurvival/Scripts/Player.lua) | Respawns without creating an inventory-drop bag. | 2/10 |
| [Spawns](BetterSurvival/Scripts/survival_spawns_3x.lua) | Triples overworld unit spawn chances. | 2/10 |
| [World](BetterSurvival/Scripts/Overworld.lua) | Boots a custom survival overworld and terrain. | 2/10 |
| [Fuel](BetterSurvival/description.json) | Metadata claims half consumption, but no implementation exists. | 0/10 |
| [Restrictions](BetterSurvival/Scripts/Game.lua) | Metadata claims none, but the current script enables them. | 0/10 |

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
