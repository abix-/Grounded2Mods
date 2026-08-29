# Police

Everything police: the measured vanilla system and the
crime_level design on top of it. Findings and design discussion
recorded 2026-08-29.

## Vanilla, live-measured

Classes: `PoliceManager` (crimes, relationship, jail),
`PoliceCrimeCoordinator` + `PoliceCrimeSettings`
(wanted/chase/arrest tuning), `PoliceBot` with actions
(`SpotCriminalAction`, `ChaseCriminalAction`,
`ArrestCriminalAction`). `tests/research_police.rs` reads the
live values; several DIFFER from the code defaults.

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

Chases start through three doors, and only one needs the meter
at 0: crimes reported near a cop (running over a pedestrian,
failed pickpocket, drive-by) trigger an on-the-spot local chase
at ANY relationship; the persistent wanted state needs
relationship 0 plus a crime; attacking a cop triggers immediate
retaliation regardless.

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
via `FighterHandler.RegenerateHealth` (the player never gets
that call).

After wanted expires nothing is remembered; the relationship
recovers on its own clock as above.

Every number above lives on the `PoliceCrimeSettings` object or
the `crimeTable`, both reachable by handle at runtime, so all of
it is tunable by a write or a Harmony prefix.

# The crime_level design

## The operator's rules

1. A crime makes you a criminal. The record does not recover by
   waiting. ("if i do a crime then im a criminal")
2. Police chase criminals ON SIGHT, no fresh crime needed.
3. How bad a criminal you are determines how hard they escalate.
4. Getting clean costs something: pay, or be arrested and serve.

Prior art: Red Dead Redemption 2's bounty and Skyrim's per-hold
bounty work exactly this way (persistent until paid, recognized
on sight). GTA V by contrast forgives on evade.

## crime_level

A second value next to the game's `relationship`. Two values
because relationship is clamped at 0 (no depth to escalate from)
and vanilla writes it (bribe +10, story resets, recovery tick).
Their jobs:

- `relationship` (vanilla's): how much the police currently
  tolerate you. Untouched.
- `crime_level` (the mod's): the sum of your UNPAID crime
  scores, using the game's own per-crime scores (PoliceKill 30,
  ClubRaid 20, Pickpocket 10, ... see research.md). Only goes up
  on crime; only goes down by paying or arrest. Persists per
  save slot.

## Rules in mechanism terms

| Rule | Mechanism |
|---|---|
| Record accumulates | Prefix on `PoliceManager.CommitCrime` adds the crime's score to crime_level |
| No passive recovery | Skip `RecoverRelationshipAfterCrimeFreeMinute` (vanilla +5/quiet-minute) |
| Chase on sight | While crime_level > 0: when `IsAnyPoliceInDetectionRange` sees the player, call the game's `AddWanted` |
| No expiry while seen | Keep `reactionWindowTimer`-style refresh of the wanted deadline while any cop has the player in range |
| Escalation | crime_level tier writes `PoliceCrimeSettings` (detectionRadius, shootingInterval, shotMissChance) and drives `SpawnPoliceTeam` / the response dispatcher |
| Pay to clear | Bribe priced off crime_level (about $10 per point, Charisma-discounted); paying reduces the record |
| Arrest clears | The jail sequence wipes crime_level: time served |

## Escalation tiers (starting values, to be tuned in play)

| crime_level | Being seen by a cop means |
|---:|---|
| 0 | Clean. Vanilla behavior |
| 1 - 25 | The cop chases; normal stats |
| 26 - 60 | Chase + one response car dispatched; wider detection, faster shooting |
| 61 - 120 | Two teams, wider still, better aim; serious bribe money |
| 120+ | Teams keep coming while seen |

## Vanilla vs GTA V vs this design

| Aspect | Vanilla (measured) | GTA V | crime_level design |
|---|---|---|---|
| Criminal identity | None; 50 s wanted timer, then forgotten | None; stars vanish on evade | PERSISTENT crime_level until paid or arrested |
| Trigger to be chased | Crime near a cop, or wanted (relationship 0 + crime) | Crime witnessed/reported | Any cop SEEING you while crime_level > 0 |
| Escalation | None; on/off | 5 stars by crime severity, response scales to NOOSE/helicopters | Tiers by crime_level depth; more teams, better stats per tier |
| Chase persistence | Give up at 21 m; 50 s flat expiry | Pursuit + search zone while seen; evade timer when hidden | No expiry while any cop sees you; search is the escalation teams converging |
| Getting away | Stand still for 50 s | Break line of sight, stay hidden | Getting away hides you, but you STAY a criminal |
| Getting clean | Wait 6 min (to 30) or flat $1000 bribe | Automatic once evaded | Pay proportional to the record, or arrest wipes it (time served) |
| Cost of being bad | Nearly zero | Momentary danger | Walking the city while deep in crime_level is genuinely dangerous |
| Who reports crimes | The game itself, silently | Witnesses and victims call it in; kill the witness fast and no report | Unchanged from vanilla (witness model out of scope) |
| Weapons used | One Colt, 10 damage, 1/s, 35% miss | Pistols to carbines to helicopter snipers, scaling with stars | Same Colt, but interval/miss/damage tuned harder per tier |
| In a vehicle | Cops keep 6 m and pot-shot; no arrest | PIT maneuvers, spike strips, roadblocks | Unchanged from vanilla (vehicle pursuit out of scope) |
| Arrest | Walk within 1.5 m while a bar fills | Cornered at gunpoint; bail + impound | Same mechanics, but arrest WIPES crime_level: time served |

## Open questions

- Should some crimes (PedestrianKill at score 1) even count, or
  is there a floor below which cops don't care on sight?
- Does arrest wipe the whole record or a served-time fraction?
- Where does the operator SEE crime_level (control plane first;
  in-game display later)?
