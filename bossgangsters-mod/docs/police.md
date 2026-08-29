# Police design: crime_level

Design discussion recorded 2026-08-29. Vanilla findings and all
measured numbers live in [research.md](research.md) section 4.

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

## Open questions

- Should some crimes (PedestrianKill at score 1) even count, or
  is there a floor below which cops don't care on sight?
- Does arrest wipe the whole record or a served-time fraction?
- Where does the operator SEE crime_level (control plane first;
  in-game display later)?
