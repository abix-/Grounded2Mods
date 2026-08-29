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

## 3. Punching bag minigame (2026-08-29, live-measured)

`PunchingBagStation` (gym, trains Fight). The prompt appears at
the bag's far apex (`ActivateNextPrompt` sets `promptActive` and
`expectsLeftHand`); A or D becomes `ResolvePunch(usedLeftHand,
crossFade)`.

How a press is graded, from `ResolvePunch`:

- `Started` when the bag is not moving (first hit).
- If a previous punch is still mid-animation and its impact has
  not landed, the press reuses that punch's
  `pendingImpactMultiplier` (it can repeat a Perfect, never
  create one).
- `Miss` when `promptActive` is false or `reactionWindowTimer`
  hit zero.
- Otherwise: `Perfect` when `signedBagAngle / maximumAwayAngle
  <= perfectWindows + perfectWindowForgiveness` for the tier,
  else `Good`. Graded at the PRESS, from the bag's position.

Live tuning values (read off the station, match the decompile
defaults): reaction windows 0.6/0.4/0.3 s, perfect windows
(forgiven) 0.22/0.15/0.10, apex hold 0.18 s, pendulum spring
10/20/30 per tier 1/2/3.

### Tier 2 wait sweep, graded by the live game

One press per prompt at every possible wait after the prompt
(`research`: the auto-hit swept 0 to 0.55 s in 0.05 steps; two
full passes gave identical grades):

| Wait after prompt | Bag ratio at press | Countdown left | Grade |
|---:|---:|---:|---|
| 0.00 | 0.81 | 0.400 | Good |
| 0.05 | 0.77 | 0.344 | Good |
| 0.10 | 0.82 | 0.291 | Good |
| 0.15 | 0.71 | 0.246 | Good |
| 0.20 | 0.81 | 0.190 | Good |
| 0.25 | 0.66 | 0.145 | Good |
| 0.30 | 0.68 | 0.096 | Good |
| 0.35 | 0.58 | 0.048 | Good |
| 0.40 | 0.43 | 0.000 | Miss |
| 0.45+ | (no press possible: the prompt is already over; the game itself scores those prompts Miss when the bag arrives) | | |

Frame capture of one full tier 2 prompt: prompt appears with the
bag at ratio ~0.71 and countdown 0.400; the bag stands still for
the 0.18 s apex hold (countdown 0.400 -> 0.212); it then returns
at roughly 0.15 ratio per 0.14 s and is still at ~0.50 when the
countdown dies. The forgiven perfect ratio (0.15) is reached
roughly a third of a second AFTER countdown death.

### Conclusion

On this build (Unity 6000.3.13f1 game version as of 2026-08-29),
tier 2's countdown and the bag's return never overlap inside the
perfect window: every possible single press grades Good (or Miss
when late). Perfect is reachable by press timing on tier 1 only.
The auto-hit therefore holds the ceiling on tiers 2 and 3 with
all-Good. Getting Perfect there would require changing game
state (for example keeping `reactionWindowTimer` alive until the
bag actually arrives), not press timing.
