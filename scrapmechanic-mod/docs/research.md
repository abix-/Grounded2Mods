# scrap mechanic modding research

## game basics

- **developer:** Axolot Games (Swedish indie studio)
- **engine:** custom "Contraption engine" (started on OGRE3D, rewrote to custom renderer in 0.2.0)
- **language:** C++ core, Lua 5.1 scripting, JSON data files
- **physics:** custom Bullet plugin
- **gui:** MyGUI
- **steam app id:** 387990
- **workshop:** yes, ~500k items

## modding layers

### layer 1: lua mods (workshop)

- official mod tool ships via Steam (Library > Tools)
- mods are Lua scripts + JSON configs packaged for Steam Workshop
- can add new items, tools, terrain tiles, crafting recipes using existing game systems
- Lua API is extensive: tools, interactables, shapes, terrain, GUI callbacks
- limitation: cannot add new game mechanics or behaviours that the engine does not already support
- this is the "safe" layer, similar to Factorio's data/control stage separation

### layer 2: dll mods (native)

- community-built DLL injection via [SM-DLL-Injector](https://github.com/QuestionableM/SM-DLL-Injector)
- DLLs placed in `Steam/steamapps/common/Scrap Mechanic/Release/DLLModules/`
- injector loads them at game start
- can hook C++ functions, extend the Lua VM, add new native features
- examples: proximity voice chat, dynamic sun, better paint tool, custom audio, force dev mode
- written in C++ (MSVC)
- no standard mod loader framework (no BepInEx, no MelonLoader, no UE4SS equivalent)
- each DLL mod rolls its own hooks via pattern scanning or hardcoded offsets

### layer 3: lua vm hooks

- some DLL mods hook the Lua VM itself to add new Lua functions callable from scripts
- [SM-CustomAudioExtension](https://github.com/QuestionableM/SM-CustomAudioExtension) is an example
- this bridges native code and the Lua scripting layer

## workshop mod structure

workshop mods follow a fixed layout created by the official Mod Tool:

```
MyMod/
  description.json          # mod metadata: name, uuid, type, version, dependencies
  Objects/
    Database/
      ShapeSets/
        myparts.json        # defines blocks and parts with UUIDs, renderables, physics
  Scripts/
    mypart.lua              # Lua class scripts attached to interactable parts
  Gui/                      # optional MyGUI XML layouts
  Effects/                  # optional particle/sound definitions
```

### description.json

```json
{
  "creatorId": 76561198...,
  "description": "My mod description",
  "fileId": 123456789,
  "localId": "40639a2c-bb9f-4d4f-b88c-41bfe264ffa8",
  "name": "My Mod",
  "type": "Blocks and Parts",
  "version": 0
}
```

mod types: "Blocks and Parts", "Custom Game", "Terrain Assets"

### shapeset json (parts definition)

```json
{
  "partList": [{
    "uuid": "a-uuid-here",
    "name": "my_widget",
    "renderable": "$CONTENT_uuid/Objects/Renderables/widget.rend",
    "color": "df7f00",
    "box": {"x": 2, "y": 1, "z": 2},
    "rotationSet": "PropYZ",
    "physicsMaterial": "Metal",
    "ratings": {"density": 4, "durability": 7, "friction": 3, "buoyancy": 8},
    "flammable": false,
    "stackSize": 50,
    "scripted": {
      "filename": "$CONTENT_uuid/Scripts/widget.lua",
      "classname": "Widget",
      "data": {}
    }
  }]
}
```

the `scripted` block connects a part to a Lua class. if present, the game creates an
instance of that class for every placed copy of the part.

### lua scripting model

dual sandbox: server runs on host, client runs on all players.

```lua
Widget = class()

function Widget:server_onCreate()
    -- runs on the server when the part is placed
end

function Widget:client_onInteract(character, state)
    -- runs on the client when a player interacts
end

function Widget:server_onFixedUpdate(dt)
    -- per-tick server logic
end
```

callbacks prefixed `server_` run only on the host. `client_` run on all machines.
network sync via `self.network:sendToClients(...)` and `self.network:sendToServer(...)`.

### lua API surface (sm.* namespace)

70+ namespaces covering the full engine:

| namespace | covers |
|-----------|--------|
| sm.shape | shape creation, destruction, properties |
| sm.body | rigid body physics |
| sm.joint | connections between bodies |
| sm.player | player state, inventory |
| sm.character | character movement, animation |
| sm.unit | AI units (enemies, NPCs) |
| sm.interactable | part logic, connections, power |
| sm.container | inventory containers |
| sm.physics | raycasts, explosions, forces |
| sm.effect | particles, sounds |
| sm.audio | spatial audio |
| sm.render | camera, rendering |
| sm.construction | building system |
| sm.game | game mode, time, spawning |
| sm.pathfinder | AI pathfinding |
| sm.terrainGeneration | terrain gen callbacks |
| sm.json | read/write JSON files |
| sm.util | general utilities |
| sm.debugDraw | debug visualization |

10 class types: ShapeClass, ToolClass, GameClass, PlayerClass, CharacterClass,
UnitClass, WorldClass, HarvestableClass, ScriptableObjectClass, ClientScriptableObjectClass

50+ userdata types: Shape, Body, Joint, Player, Character, Unit, Container, etc.

### what workshop mods CAN do

- add new blocks and parts (with custom meshes, physics, scripts)
- add new tools (ToolClass with custom logic)
- create custom game modes (GameClass with win conditions, timers, spawn rules)
- create custom terrain (WorldClass + terrainGeneration callbacks)
- modify harvestable objects (drops, behaviour)
- add GUI elements (MyGUI XML)
- play effects, sounds, particles

### what workshop mods CANNOT do

- add new engine features (new physics types, new rendering, new networking)
- modify core game systems (crafting progression, save format, multiplayer protocol)
- hook or replace existing game functions
- access the filesystem beyond sm.json
- run native code

## community infrastructure

- [Scrap-Mods org on GitHub](https://github.com/Scrap-Mods): modding libraries and tools
  - **SmSdk**: C++ SDK for making DLL mods (function signatures, class layouts)
  - **networking-fix**: DLL mod that fixes packet stalling
  - **websocket**: C++ DLL that exposes WebSocket API to Lua scripts
  - **http**: C++ DLL that exposes HTTP API to Lua scripts
  - **LuaObject**: base64 Lua data serializer for persistent storage
- [QuestionableM on GitHub](https://github.com/QuestionableM): most active DLL modder
- [Thunderstore page](https://thunderstore.io/c/scrap-mechanic/) exists but secondary to Workshop
- Steam Workshop is the primary distribution channel for Lua mods
- DLL mods distributed via GitHub releases

### SM-DLL-Injector details

- not a traditional injector (no CreateRemoteThread/LoadLibrary)
- works by DLL replacement: ships a fake `vcruntime140_1.dll` that proxies to the real one
- the fake DLL loads everything in the `DLLModules/` folder at game startup
- disable with `-noinject` launch arg
- this is a DLL proxy/sideload, not injection. clean, no anti-cheat concerns

## does modforge fit?

### what modforge does

modforge is a Rust DLL that gets injected into a running game process. it pattern-scans the
game binary to find function addresses, hooks them, and exposes an op-based HTTP API for
real-time game manipulation. it currently supports:
- UE4/UE5 games via ueforge (UE4SS ecosystem)
- Unity Mono games via unityforge (BepInEx/MelonLoader ecosystem)

### fit assessment

| factor | verdict | notes |
|--------|---------|-------|
| custom C++ engine | partial fit | no UObject or Mono reflection to lean on. all hooks are raw pattern scans, similar to horsey-mod or grounded2-mod approach |
| Lua 5.1 scripting | good fit | could hook the Lua VM to register new functions, read/write game state through Lua |
| DLL injection | good fit | SM-DLL-Injector proves the path works. modforge's injector harness can target this |
| no mod framework | neutral | no BepInEx/MelonLoader means no free hooking infrastructure, but also no framework conflicts |
| pattern scanning | good fit | modforge::patterns::sleuth handles this already |
| Windows only | good fit | modforge is Windows-only anyway |

### approach options

1. **pure native (like horsey-mod):** pattern scan the Contraption engine binary, hook C++ functions directly. most power, most reverse engineering effort.

2. **lua vm bridge:** hook the Lua VM (lua_pcall, luaL_register, etc.) to inject new Lua functions that call back into modforge's Rust code. game scripts can then call modforge ops from Lua. less RE needed for gameplay modding since the Lua API already exposes a lot.

3. **hybrid:** native hooks for engine internals (rendering, physics, networking) + Lua VM bridge for gameplay logic. this is probably the right answer.

## custom game mode: what you can do

custom game mode is the most powerful modding layer without DLLs. your GameClass script
is a singleton that controls the entire session. combined with WorldClass, PlayerClass,
and UnitClass, you can build a completely different game inside the engine.

### game rules (GameClass constants)

| constant | controls | default |
|----------|----------|---------|
| defaultInventorySize | player inventory slots | 40 |
| enableLimitedInventory | stack size limits | false |
| enableRecipes | locked recipes needing discovery | true |
| enableAggro | enemy aggression | true |
| enableFuelConsumption | fuel depletion | false |
| enableAmmoConsumption | ammo depletion | false |
| enableRestrictions | build placement rules | false |
| enableUpgrade | part upgrades | false |

### world control (WorldClass)

25 constants controlling the world. highlights:

- **terrain:** enableVoxelTerrain, enableSurface, enableAssets, enableClutter, terrainScript
- **building rules:** enableBuildOnSurface, enableBuildOnBodies, enableBuildOnAssets, enableBuildOnLift
- **world type:** isIndoor, isStatic, renderMode ("outdoor", "challenge", "warehouse")
- **bounds:** cellMinX/maxX, cellMinY/maxY
- **navigation:** enableNodes, enableNavMesh

world callbacks give you hooks into:
- terrain generation and cell streaming (server_onCellCreated, server_onCellLoaded, etc.)
- projectile impacts, melee hits, explosions, collisions
- voxel destruction and construction (mining, digging)
- interactable creation and destruction
- mining loot spawns (server_onMining with spawn candidates you can filter)

### player control (PlayerClass)

per-player instance with callbacks for:
- input events (interact, cancel, reload)
- damage (projectile, explosion, melee, collision, crush)
- inventory changes (server_onInventoryChanges with uuid + difference)
- shape removal tracking (what the player dismantled)
- world transitions

### AI units (UnitClass)

server-side AI controlling a Character. callbacks for:
- per-tick updates + a separate server_onUnitUpdate for heavy AI decisions
- combat (projectile, explosion, melee, collision, crush)
- persistent save/load (isSaveObject constant)
- uses sm.pathfinder for navigation

### what you could build with custom game mode alone

- **expanded survival:** 1000-slot inventory, different fuel/ammo rules, custom recipes
- **tower defense:** custom terrain, AI waves via UnitClass, score tracking
- **racing:** custom world with checkpoints, timer, restricted building
- **PvP arena:** player damage callbacks, team tracking, respawn logic
- **RPG:** custom AI units with dialogue (client_onInteract), quests via storage, XP via counters
- **factory sim:** custom crafting chains, resource processing, logistics puzzles
- **exploration:** procedural terrain via terrainScript, mining loot tables, discoveries
- **combat overhaul:** custom projectile damage, melee power, explosion radius, aggro rules

### the survival mode limitation

all of the above only works in Custom Game mode. Survival mode ships with its own
GameClass/WorldClass/PlayerClass scripts that are baked into the game content. workshop
mods cannot replace them. you play "custom survival" not actual survival.

to modify real Survival: DLL mod territory (patch the engine, replace the scripts in memory,
or hook the Lua VM to override constants after the survival scripts load).

## open questions

- [ ] what does the game binary look like? (PE structure, protections, symbol stripping)
- [ ] is the Lua VM statically linked or a separate DLL?
- [ ] what Lua C API functions are exported or findable by pattern?
- [ ] does the game use any anti-cheat or integrity checks?
- [ ] what are the most interesting things to mod? (need to play the game first)
- [ ] does SM-DLL-Injector conflict with modforge's own injection, or can they coexist?
- [ ] is there a community RE effort with known function signatures?

## next steps

1. play the game, understand what it does
2. look at the game binary (PE analysis, exports, Lua symbols)
3. study existing DLL mods (SM-DynamicSun, SM-BetterPaintTool) for hook patterns
4. decide on approach (native vs lua bridge vs hybrid)
5. set up the scrapmechanic-mod crate
