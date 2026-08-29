# The XP skill system

How The Boss Gangsters Nightlife levels up skills. Read from the
ilspycmd decompile of Assembly-CSharp, 2026-08-29. Class and
method names below are the game's own; none of this is live
verified yet unless it says so.

## The eight skills

The `FighterSkill` enum (FighterSkill.cs):

Power, Fight, Vision, Charisma, Pickpocket, Craft, Trade,
Lockpicking.

## Where a skill lives

Every fighter (your character and every gangster) carries an
array of eight `EmployeeAbility` entries (Tycoon/EmployeeAbility.cs),
indexed by the enum:

```csharp
public class EmployeeAbility
{
    public string abilityKey;    // for example "gangster_info_lockpick"
    public int abilityValue;     // the LEVEL, 1..10
    public float progressValue;  // the XP toward the next level
    public int GetNextLevelRequired() => 50 + abilityValue * 50;
}
```

So the XP needed for the next level depends only on the current
level:

| Level | XP to next |
|---:|---:|
| 1 | 100 |
| 2 | 150 |
| 3 | 200 |
| ... | ... |
| 9 | 500 |
| 10 | capped, no more XP accepted |

## How XP is added

One method does all leveling: `FighterHandler.AddSkillXp(skill, xp)`
(FighterHandler.cs). In order:

1. If the fighter is not yours (`IsOurFighter` false) or the
   skill is already level 10, do nothing.
2. Multiply the XP by `GetSkillExperienceMultiplier(skill)`.
3. Add it to `progressValue`.
4. If `progressValue` passed `GetNextLevelRequired()`, subtract
   the requirement, add one to `abilityValue`, and play the
   level-up animation. Leftover XP carries over.
5. Leveling Power also immediately recomputes max health
   (see below).

There is no character level and no shared XP pool; each skill
levels alone.

### The XP multiplier

Each fighter has at most ONE bonus skill
(`fighterData.experienceBonusSkillIndex`) with a x2 or x3
multiplier (`experienceBonusMultiplier`). It is rolled once when
a hire offer is generated (`RollHireOfferExperienceBonus`): 10%
chance of x3, otherwise x2, on a random one of the eight skills.
Every other skill earns XP at x1. The office UI shows the bonus
in a different color per multiplier.

## Where XP comes from

Every call site of `AddSkillXp` in the game:

| Skill | Activity | Source file |
|---|---|---|
| Power | Bench press station in the gym | BenchPressStation.cs |
| Power | TAKING damage: half the damage as XP, only if it did not kill you | FighterHandler.cs (`AddSkillXp(Power, amount / 2f)`) |
| Fight | Punching bag station in the gym | PunchingBagStation.cs |
| Fight | Dealing melee weapon damage: damage / 5 as XP | Weapon.cs |
| Vision | Dealing ranged weapon damage: damage / 5 as XP | Weapon.cs |
| Vision | Shooting range | ShootingRangeManager.cs |
| Charisma | Dock fights | DockFightManager.cs |
| Charisma | Security minigame (5 XP boss, 1 XP per point) | SecurityMiniGameBoss.cs, SecurityPoint.cs |
| Charisma | Successful reveal while selling weed | TheBoss.WeedSelling/SellingPoint.cs |
| Pickpocket | Pickpocketing pedestrians | PedestrianClickHandler.cs |
| Craft | Lab table craft collection | LabTableCraftStation.cs |
| Craft | Crafting table, flat 10 XP per craft | Project.Crafting/CraftingTableManager.cs |
| Trade | Hot dog minigame correct sale | HotDogMinigameHandler.cs |
| Trade | Shop sales | Project.Inventory/ShopManager.cs |
| Trade | Successful weed sale | TheBoss.WeedSelling/SellingPoint.cs |
| Lockpicking | Lockpicking minigame, score x 4 | LPLockpicking.cs |
| any | Gym training results generally | GymManager.cs (`result.RewardSkill, result.RewardXp`) |
| any | Office jobs reward skill XP and show level-up notifications | Tycoon/GangsterOfficeJobManager.cs |

## What levels do

Reads go through `FighterHandler.GetSkillLevel(skill)`, which
returns `abilityValue`, except Vision can be temporarily
overridden (`SetTemporaryVisionSkillLevel`, clamped 1..10; some
activity lends you a vision level).

Effects found so far:

| Skill | Effect | Where |
|---|---|---|
| Power | Max health = default + level x 20 | FighterHandler (in `AddSkillXp` and hire generation) |
| Power | Max stamina = 100 + level x 10 | FighterHandler.cs `MaxStamina` |
| Vision | `GetFocusAbility()` = level x 0.05 | FighterHandler.cs |
| Charisma | Most-read skill (10 call sites), gating social outcomes | various |

The rest of the reads are spread one or two per skill across
their activities; not yet cataloged.

For gangsters working stations, `EmployeeBase.CurrentAbilityValue`
clamps `abilityValue + abilityDebuffValue` to 1..10, so an
unhappy worker can perform below their trained level
(Tycoon/EmployeeBase.cs).

## Where starting levels come from

- **Your character:** at character creation you allocate 12
  points across the skill sliders (`CharacterSkillUI`, global
  namespace; `SkillSlider` in Tycoon). The values are copied
  into `employeeAbilities[j].abilityValue`
  (CharacterCreateManager.cs).
- **Hired gangsters:** `GenerateHireOfferAbilities` rolls 5 of
  the 8 skills to share a random 5..20 total points, zeroes the
  rest, sets health from Power, and rolls the XP bonus skill.

## Persistence

`abilityValue` and `progressValue` are serializable fields on
the fighter's data, saved with the save file (SaveManager,
`Slot_0..3` under the game's LocalLow folder).

## Modding notes

Everything is plain fields and one public method on
`FighterHandler`, reachable over the control plane with the
existing ops: read `employeeAbilities` via `read_field`, grant
levels by calling `AddSkillXp` via `invoke_method`, or write
`abilityValue` directly (remember Power's health/stamina only
recompute inside `AddSkillXp`, so a raw Power write skips them).
