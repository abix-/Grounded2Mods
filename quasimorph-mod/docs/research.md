# quasimorph mod research

## engine

- unity (mono, not il2cpp)
- first-party mod API: game loads C# DLLs directly from Mods/ folder
- no MelonLoader or BepInEx needed

## mod structure

every mod needs three files in a content folder:
- `modmanifest.json` (generated via console command `mod_createmanifest`)
- compiled `.dll`
- `thumbnail.png`

## hook types

all hooks take `IModContext context`. namespace is `MGSC`.

### init
- `BeforeBootstrap`: before game init
- `AfterConfigsLoaded`: after configs load during init
- `AfterBootstrap`: after init, before main menu
- `MainMenuStarted` / `MainMenuDestroyed`

### dungeon (tactical missions)
- `DungeonStarted` / `DungeonFinished`
- `DungeonUpdateBeforeGameLoop` / `DungeonUpdateAfterGameLoop`

### space (ship management)
- `SpaceStarted` / `SpaceFinished`
- `SpaceUpdateBeforeGameLoop` / `SpaceUpdateAfterGameLoop`

### special (different signatures)
- `ResourcesLoad`: returns `UnityEngine.Object`
- `BeforeSaveLoaded`, `BeforeDungeonLoaded`, `BeforeSpaceLoaded`: accept `JSONNode`

## IModContext

- `context.State`: access game modules/singletons
- `context.ModContentPath`: path to this mod's folder on disk

## console commands

- `mod_createmanifest <name> <path>`: generate modmanifest.json
- `mod_createworkshopitem <path>`: publish to workshop
- `mod_updateworkshopitem <id> <path> <update_thumbnail>`: update
- `listmod`: show active mods

## open questions

- what does `context.State` actually expose? need to decompile or inspect at runtime
- which .NET framework version? assumed net48 (standard unity mono), needs verification
- exact game exe name and data folder name (assumed Quasimorph.exe, Quasimorph_Data)
