# The Boss Gangsters Nightlife research

Game facts verified 2026-08-29 unless noted.

## The game

| Fact | Value |
|---|---|
| Steam AppId | 2774040 |
| Exe | `TheBossGangsters.exe` |
| Install | `C:\Games\Steam\steamapps\common\The Boss Gangsters Nightlife` |
| Developer | BefGames (from `app.info`) |
| Engine | Unity 6000.3.13f1, Mono scripting backend |
| Mod loader | None shipped. BepInEx 5.4.23.5 installed by hand. **5.4.23.2 crashes this Unity build on launch** (Doorstop 4.3 too old; the 4.5.0.0 + 5.4.23.5 set from How to Fish works). |
| Control plane | port 17176 |
| Player log | `%USERPROFILE%\AppData\LocalLow\BefGames\TheBossGangsters\Player.log` |
| Saves | `%USERPROFILE%\AppData\LocalLow\BefGames\TheBossGangsters\Slot_0..3` |
| `Mods\Radio\` | Empty folder the game creates in its install dir; looks like a custom radio music drop, not a code mod loader. |

## 1. Decompile (2026-08-29)

`ilspycmd -p -o <outdir> ...\Managed\Assembly-CSharp.dll` gives
1992 C# files. The game's own code lives mostly in the `Tycoon`,
`TheBoss.*`, `Project.*`, and `_Project._Scripts*` namespaces;
the rest is asset packs (Gley traffic/pedestrians, Synty
locomotion, ArcadeVP, Michsky UI, RayFire).

Singletons follow `MonoSingleton<T>`, reachable via
`MonoSingleton<T>.Instance`.

## 2. Player, money, game manager (research_managers)

Confirmed live over the control plane, `research_managers.rs`:

```text
MoneyManager: live instance, handle 4
GameManager: live instance, handle 6
MoneyManager.money = 500
ClubPlayer.playerBot = {"handle":11,"name":"PlayerBot(Clone)","type":"PlayerBot"}
find_instances(ClubPlayer): 1 instance(s)
```

| Concern | Class | Notes |
|---|---|---|
| Player | `ClubPlayer : MonoSingleton<ClubPlayer>` (namespace `Tycoon`) | Field `playerBot` (a `PlayerBot : BotBase`, global namespace) is the player character; referenced 135 times across the game's code. Also `playerFighterHandler` (60 references). |
| Money | `MoneyManager : MonoSingleton<MoneyManager>` (namespace `Tycoon`) | `[SerializeField] private int money`. Methods `AddMoney(int, Bill, bool forceState = false)`, `SpendMoney(int, Bill)`, `AddMoneyForEditor(int)`. |
| Game state | `GameManager : MonoSingleton<GameManager>` (namespace `Tycoon`) | `CurrentGameState` property plus a `gameStateChanged` `Action<GameState>` other managers subscribe to. |
