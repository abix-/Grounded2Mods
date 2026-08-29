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

Extended to the WHOLE timing space (waits 0 to 1.20 s, pressing
even after the prompt ends; two passes, identical grades):

| Wait | State at press | Grade |
|---:|---|---|
| 0.00 - 0.40 | prompt on, countdown alive, bag ratio 0.46 - 0.82 | Good |
| 0.45 - 0.55 | prompt on, countdown DEAD, bag ratio 0.06 - 0.34 | Miss |
| 0.60 - 1.05 | prompt over, bag arrived | Miss |
| 1.10 - 1.20 | bag stopped, next swing | Started |

The decisive line: `wait 0.55s ... ratio 0.086 timer 0.000 ->
Miss`. The bag WAS inside the perfect window (0.15); the press
graded Miss anyway because `reactionWindowTimer` was already
zero.

Frame capture of one full tier 2 prompt: prompt appears with the
bag at ratio ~0.71 and countdown 0.400; the bag stands still for
the 0.18 s apex hold (countdown 0.400 -> 0.212); it then returns
at roughly 0.15 ratio per 0.14 s and is still at ~0.50 when the
countdown dies. The forgiven perfect ratio (0.15) is reached
roughly a third of a second AFTER countdown death.

## 4. Police (2026-08-29, live-measured)

Classes: `PoliceManager` (crimes, relationship, jail),
`PoliceCrimeCoordinator` + `PoliceCrimeSettings`
(wanted/chase/arrest tuning), `PoliceBot` with actions
(`SpotCriminalAction`, `ChaseCriminalAction`,
`ArrestCriminalAction`). `research_police.rs` reads the live
values; several DIFFER from the code defaults.

### The relationship meter drives everything

`PoliceManager.relationship`, 0..100, starts 50. Every crime
subtracts its score (subject to a per-crime cooldown). You only
become WANTED when the meter is at 0 and you commit a crime with
a known criminal. Recovery paths:

- +5 per crime-free minute, but only up to 30
  (`RecoverRelationshipAfterCrimeFreeMinute`).
- Bribe at the police station: $1000 for +10, discounted up to
  50% by Charisma. NOTE: the discount clamps Charisma to 10, so
  the mod's 100-level cap does not extend it.
- Story resets put it back to 50.

Per-crime scores, read live off the running game:

| Crime | Score | Cooldown |
|---|---:|---:|
| PoliceKill | 30 | 0 s |
| ClubRaid | 20 | 0 s |
| IllegalDrinkFatality | 15 | 0 s |
| DrugDealer | 10 | 30 s |
| CarSteal / TributeCapture / Pickpocket / VehicleExplosion | 10 | 0 s |
| PoliceShoot | 10 | 2 s |
| DrugDealAttempt | 5 | 0 s |
| TaxiScam | 5 | 5 s |
| PedestrianKill | 1 | 0 s |
| HumanTrafficker | 0.3 | 0 s |

### Wanted, chase, arrest (live settings)

| Setting | Live value | Code default |
|---|---:|---:|
| wantedDuration | 50 s | 90 s |
| detectionRadius | 20 m | 15 m |
| chaseCooldownDuration | 20 s | 60 s |
| arrestDistance | 1.5 m | 1.5 m |
| shootingStartDelay | 5 s | 5 s |
| shootingInterval | 1.0 s | 1.25 s |
| shotDamage | 10 | 10 |
| shotMissChance | 0.35 | 0.30 |

Flow: wanted lasts 50 s. A police bot within 20 m spots and
chases (repathing to you every 0.3 s); step outside 20 m and
that bot gives up. Within reach it fills an arrest progress bar
scaled between chase start distance and 1.5 m; at 1.5 m you are
arrested (jail sequence, seats in the jail, bail priced per
captivity day per fighter). During a chase, after 5 s the bot
draws a Colt and shoots every 1.0 s for 10 damage with a 35%
miss chance. If you are in a vehicle they keep 6 m distance and
shoot instead of arresting. Player state machine:
Safe/InRange/Chased/Searching/Escaping/Wanted
(`PoliceRangeState`). NPCs searching for you regenerate health
(section: passive regen).

Every number above lives on the `PoliceCrimeSettings` object or
the `crimeTable`, both reachable by handle at runtime, so all of
it is tunable by a write or a Harmony prefix.

### Comparison with GTA V's wanted system

Left column live-measured above; right column from general
knowledge of GTA V's design, not its code.

| Aspect | The Boss Gangsters Nightlife | GTA V |
|---|---|---|
| Escalation | None. Wanted is on or off | 5 stars; response scales from beat cops to helicopters, roadblocks, NOOSE/FBI teams |
| What triggers it | Relationship meter must first grind to 0, then any identified crime | Instant per crime, severity-based: one star for a punch, three for killing a cop, five for a rampage |
| Who reports crimes | The game itself, silently | Witnesses and victims call it in; kill the witness fast and no report happens |
| Detection | Fixed 20 m radius per cop, no line of sight | Line-of-sight and awareness model; cops must actually see you, peds point you out |
| Pursuit | Single cop chases; step 21 m away and he gives up | Coordinated pursuit: cars cut you off, units respawn ahead of you, helicopter tracks from above |
| Searching | 20 s cooldown, then nothing | Search phase with a shrinking zone around your last seen position; leaving line of sight starts the evade timer, being spotted resets it |
| Duration | Flat 50 s and it just expires | No timer while seen; evading takes real effort and scales with stars |
| Weapons used | One Colt, 10 damage, 1 shot per second, 35% miss | Pistols to carbines to snipers from the helicopter, scaling with stars |
| In a vehicle | Cops keep 6 m and pot-shot; no arrest possible | PIT maneuvers, spike strips, roadblocks, shooting out tires, dragging you out of the car |
| Arrest | Walk within 1.5 m while a bar fills | On foot at gunpoint when cornered; busted costs bail and impounds your car |
| Clearing it | Wait 50 s, or bribe $1000 at the station | Evade by hiding, respray, Lester call, or die/get busted |
| Aftermath | Meter regens +5 per quiet minute up to 30 | Cops remember nothing once evaded; stars fully gone |

The structural difference: GTA treats wanted as a pursuit
simulation (seen versus unseen, escalation, coordination); this
game treats it as a timer with a radius. The three cheapest
changes that would close most of the gap, all reachable through
the settings and classes mapped above: escalation tiers driven
by the relationship meter, a real search phase around the last
known position instead of the 21 m give-up, and no flat expiry
while any cop can see you.

### Conclusion (punching bag)

On this build (Unity 6000.3.13f1 game version as of 2026-08-29),
tier 2's countdown and the bag's return never overlap inside the
perfect window: every possible single press grades Good (or Miss
when late). Perfect is reachable by press timing on tier 1 only.
The auto-hit therefore holds the ceiling on tiers 2 and 3 with
all-Good. Getting Perfect there would require changing game
state (for example keeping `reactionWindowTimer` alive until the
bag actually arrives), not press timing.
