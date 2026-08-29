# Gangs and territory

The rival families and the territory system. Findings recorded
2026-08-29; live values from `tests/research_gangs.rs` against
the running game.

## The families

`FamilyType`: None, **ViceFamily**, **KurohanaFamily**, Player.
Two rival gangs plus you. `FamilyManager` (3173 lines) tracks a
per-family relationship the player can raise by completing
family requests (deliver N of an item for money, +5 prestige,
and relationship). Families are also where gangster hire offers
and family jobs live.

## The territory map, live

11 territories. Each has a star level (1..3), an initial owner
family, and its content: tribute places, weed selling points,
the dealer drugs that sell best there, and a thief profile.

| # | Territory | Stars | Owner |
|---:|---|---:|---|
| 0 | Palm Beach | 3 | ViceFamily |
| 1 | Hotel District | 3 | ViceFamily |
| 2 | Market District | 2 | ViceFamily |
| 3 | The Suburbs | 1 | ViceFamily |
| 4 | Red Light District | 1 | ViceFamily |
| 5 | Midtown | 2 | ViceFamily |
| 6 | Financial District | 2 | KurohanaFamily |
| 7 | The Blocks | 2 | KurohanaFamily |
| 8 | Industrial District | 3 | KurohanaFamily |
| 9 | The Outskirts | 2 | KurohanaFamily |
| 10 | The Docks | 3 | KurohanaFamily |

(Snapshot 2026-08-29: player owns none, prestige 10.)

## Prestige: the territory currency

`PlayerPrestigeManager.CurrentPrestigePoints`. Claims cost
prestige, not money: `ClaimCost = StarLevel * 100`.

Earn it by:

| Source | Prestige |
|---|---:|
| Capturing a tribute place | +5 |
| Completing a family request | +5 |
| Successful interrogation | +5 |
| Gangster office jobs | per job tier |
| Owned territories, DAILY | +10 per star per territory |

Owned territories paying daily prestige is the engine: taking
The Suburbs (1 star) pays 10/day toward the next claim.

## Taking a territory

1. **Capture every location in it.** Each tribute place and
   selling point is individually captured by fighting the
   owners' crew there (your assigned gangsters fight; captures
   are `TributeCapture` crimes as far as the police care).
2. **The Claim button appears** only when ALL locations in the
   territory are player-captured (`AreAllLocationsCapturedByPlayer`).
3. **Pay the claim**: StarLevel x 100 prestige (refundable if
   canceled before the battle).
4. **Claim phases** (`TerritoryClaimPhase`): None -> Paid ->
   Countdown -> Battle. You pick participants (your gangsters),
   a countdown runs, then the battle.
5. **The claim battle**: 3 waves of 4 enemy fighters from the
   owning family (`WaveEnemyCounts = [4,4,4]`, mixed weapon
   tiers, arriving by vehicle when the spawn route allows).
   Lose and there is a fail-recovery timer; win and the
   territory is yours (`CompleteClaimBattle`), joining the daily
   prestige payroll.

## Raids: the gangs push back

Captured locations get raided by the family they were captured
from (`ResolveRaidFamilyType`).

- **Schedule**: every EVEN in-game day, ONE random eligible
  tribute place is picked for a raid at 30% through the day
  (`TributePlaceManager.enemyRaidTimeRatio = 0.3`). Selling
  points raid at 60% of the day
  (`SellingPointEnemyRaidTimeRatio = 0.6`).
- **Warning**: a hired lookout at the place fires a notification
  earlier in the day with a camera jump to the spot; no lookout,
  no warning.
- **The raid**: 2 enemy raiders (`EnemyRaidFighterCount`) with a
  weapon tier bonus walk in and fill a capture bar
  (`raidCaptureBar`); your assigned fighters defend. Raider
  state (health, capture progress) is saved and restored across
  save/load (`TerritoryRaidEncounterSaveData`).
- Fighters assigned to a place level up by defending
  (`IncreaseFighterLevel`).

## Modding notes

Everything is singleton-reachable: `TerritoryManager` (states,
claim flow, `TerritoryStateChanged` event),
`PlayerPrestigeManager` (Add/Spend, plus a debug
`AddDebugPrestige100`), `FamilyManager`, `TributePlaceManager`
(raid scheduling per `DayEvent` on `TimeHandler`). Raid
frequency, raider count, wave sizes, claim costs, and daily
prestige are all constants or serialized fields; the same
patch-and-write toolset used for police applies.
