// Upgrades.cs. Settlement upgrades: multi-track per-structure
// upgrade state and effect patches, written against the real game
// types (docs/plans/2026-07-11-settlement-upgrades.md).
//
// Task 1 scope: the seed-keyed sidecar store, the Reinforce
// effect (a Harmony postfix on Prop.GetMaxDamage multiplying the
// type's max hit points by the placed structure's track bonus),
// and the probe entry the Rust upgrade_probe op drives. The
// action-menu patches (population, click dispatch, label) land
// in Task 2.
//
// State lives HERE (the C# side) because the effect patches run
// inside the game's hot stat reads; the Rust side reads the same
// sidecar file for its ops and tie-ins. Structures never stack
// and their ids persist in saves, so per-instance state is safe
// (unlike items, whose stacking merges instances).

using System;
using System.Collections.Generic;
using System.IO;
using HarmonyLib;
using Newtonsoft.Json.Linq;
using Unityforge.Shim;

public static class SettlementUpgrades
{
    // ---- knobs -------------------------------------------------------------
    // Reinforce: extra max hit points per level, diminishing:
    // level 1 adds ReinforceBase, each further level adds
    // ReinforceDecay of the previous step. Unbounded levels; the
    // rising cost curve (Task 2) is the practical stop.
    private const float ReinforceBase = 0.5f;
    private const float ReinforceDecay = 0.85f;

    // Cost: the structure's own repair resource,
    // ceil(RepairResourceNeeded) * CostFactor * next level.
    private const int CostFactor = 2;
    // Skill gate: Construction >= RepairSkillNeeded + level band.
    private const int LevelsPerSkillBand = 3;

    public const string TrackReinforce = "Reinforce";
    public const string TrackHealthRegen = "Health Regen";
    public const string TrackExpand = "Expand";
    public const string TrackSpikes = "Spikes";
    public const string TrackSpeed = "Speed";
    public const string TrackProductivity = "Productivity";
    public const string TrackEfficiency = "Efficiency";
    public const string TrackQuality = "Quality";
    public const string TrackSecure = "Secure";
    public const string TrackWatch = "Watch";
    // Settlement-wide tracks: keyed by COMMUNITY, not by structure,
    // and bought at the Command Post hub (one placed prop per camp).
    // For effects that have no single structure to live on.
    public const string TrackYield = "Yield";

    // Health Regen: hit points healed per minute per level.
    private const float RegenHpPerMinPerLevel = 0.2f;
    private const float RegenTickSecs = 15f;
    // Spikes: damage dealt to a melee attacker per hit per level
    // (capped at level 10).
    private const float SpikeDamagePerLevel = 0.05f;
    // Speed: extra craft progress; Productivity/Efficiency:
    // chance per craft (capped).
    private const float SpeedBase = 0.25f;
    private const float SpeedDecay = 0.9f;
    private const float ProductivityChancePerLevel = 0.04f;
    private const float EfficiencyChancePerLevel = 0.04f;
    private const float CraftChanceCap = 0.5f;
    // Secure: chance a hostile taking (the mod's theft, predation,
    // and tribute acts) finds this storage's locks holding. Capped
    // below 1 so stores are never fully theft-proof (brutal but
    // survivable cuts both ways).
    private const float SecureBlockChancePerLevel = 0.05f;
    private const float SecureBlockCap = 0.5f;
    // Watch: extra sight-range tiles per level for the guard
    // occupying the tower. The game clamps total sight to 31
    // (Character.GetSightRange; base is 15), which is the natural
    // diminishing cap.
    private const int WatchTilesPerLevel = 2;
    private const int SightRangeCap = 31;
    // Yield (settlement-wide): each level lifts a camp's crop max
    // yield, diminishing.
    private const float YieldBase = 0.35f;
    private const float YieldDecay = 0.85f;

    // Sentinel menu action ids, far above the vanilla enum range
    // (the game's switches ignore unknown values; our caption
    // prefix intercepts before any array indexes by action).
    private const int SentinelBase = 9000;
    // A separate range for settlement-wide entries so the click
    // dispatch tells them apart (community-keyed, not prop-keyed).
    private const int SettlementSentinelBase = 9500;

    /// One upgrade track: its state key, its menu action id, and
    /// which structures it applies to (by what the building does).
    private struct TrackDef
    {
        public string Name;
        public Func<Prop, bool> Applies;
    }

    private static readonly TrackDef[] TrackDefs =
    {
        new TrackDef { Name = TrackReinforce, Applies = _ => true },
        new TrackDef { Name = TrackHealthRegen, Applies = _ => true },
        new TrackDef
        {
            Name = TrackExpand,
            Applies = p => p.GetPropPrototype().MaxInventoryWeight > 0f,
        },
        new TrackDef
        {
            Name = TrackSpikes,
            Applies = p => p is Gate
                || (p.GetPropPrototype().Category != null
                    && p.GetPropPrototype().Category.Contains("Fences")),
        },
        new TrackDef { Name = TrackSpeed, Applies = p => p is CraftingProp },
        new TrackDef { Name = TrackProductivity, Applies = p => p is CraftingProp },
        new TrackDef { Name = TrackEfficiency, Applies = p => p is CraftingProp },
        new TrackDef { Name = TrackQuality, Applies = p => p is CraftingProp },
        // Appended last: sentinel action ids are SentinelBase +
        // index, so existing tracks keep their ids.
        new TrackDef
        {
            Name = TrackSecure,
            Applies = p => p.GetPropPrototype().MaxInventoryWeight > 0f,
        },
        new TrackDef
        {
            Name = TrackWatch,
            Applies = p => p is WatchTower || p is ConcreteWatchTower,
        },
    };

    /// Settlement-wide tracks, hosted by the Command Post and keyed
    /// by community id. No per-structure predicate: they belong to
    /// the whole camp.
    private static readonly string[] SettlementTracks = { TrackYield };

    /// The Command Post hub prop (story/Props/CommandPost.xml).
    private static bool IsCommandPost(Prop prop)
    {
        var proto = prop == null ? null : prop.GetPropPrototype();
        return proto != null
            && (proto.Name == "CommandPost" || proto.NativeName == "Command Post");
    }

    private static Harmony _harmony;
    private static bool _installed;

    // prop id -> track -> level. Loaded per save seed.
    private static readonly Dictionary<int, Dictionary<string, int>> Tracks
        = new Dictionary<int, Dictionary<string, int>>();
    // community id -> track -> level (settlement-wide upgrades).
    private static readonly Dictionary<int, Dictionary<string, int>> CommunityTracks
        = new Dictionary<int, Dictionary<string, int>>();
    private static long _seed;
    private static bool _loaded;

    /// Called from Main.Load (idempotent). Installs the effect
    /// patches; the store loads lazily once a session is up.
    public static void Install()
    {
        if (_installed) return;
        try
        {
            _harmony = new Harmony("abix.survivalist.upgrades");
            _harmony.Patch(
                AccessTools.Method(typeof(Prop), nameof(Prop.GetMaxDamage)),
                postfix: new HarmonyMethod(AccessTools.Method(typeof(SettlementUpgrades), nameof(GetMaxDamagePostfix))));
            // The menu: population (append our entries for the
            // hovered structure), caption (our label text; also
            // shields the caption array from sentinel indexes),
            // and click dispatch (perform the upgrade, skip the
            // vanilla switch).
            _harmony.Patch(
                AccessTools.Method(typeof(GameCursor), nameof(GameCursor.GetAvailableActions)),
                postfix: new HarmonyMethod(AccessTools.Method(typeof(SettlementUpgrades), nameof(GetAvailableActionsPostfix))));
            _harmony.Patch(
                AccessTools.Method(typeof(AvailableAction), nameof(AvailableAction.GetCaption)),
                prefix: new HarmonyMethod(AccessTools.Method(typeof(SettlementUpgrades), nameof(GetCaptionPrefix))));
            _harmony.Patch(
                AccessTools.Method(typeof(Hud), nameof(Hud.OnSelectedAction)),
                prefix: new HarmonyMethod(AccessTools.Method(typeof(SettlementUpgrades), nameof(OnSelectedActionPrefix))));
            // The track effects: capacity (Expand), attacker
            // bleed (Spikes), craft speed, extra product
            // (Productivity), ingredient refund (Efficiency).
            // Health Regen needs no patch (a slow tick tends the
            // public per-instance damage field); Quality hands
            // its bonus to the Rust craft roll.
            _harmony.Patch(
                AccessTools.Method(typeof(Prop), nameof(Prop.GetMaxInventoryWeight)),
                postfix: new HarmonyMethod(AccessTools.Method(typeof(SettlementUpgrades), nameof(GetMaxInventoryWeightPostfix))));
            _harmony.Patch(
                AccessTools.Method(typeof(Prop), nameof(Prop.ApplyDamage)),
                postfix: new HarmonyMethod(AccessTools.Method(typeof(SettlementUpgrades), nameof(ApplyDamagePostfix))));
            _harmony.Patch(
                AccessTools.Method(typeof(CraftingProp), nameof(CraftingProp.Craft)),
                postfix: new HarmonyMethod(AccessTools.Method(typeof(SettlementUpgrades), nameof(CraftPostfix))));
            _harmony.Patch(
                AccessTools.Method(typeof(Recipe), nameof(Recipe.CreateProduct)),
                postfix: new HarmonyMethod(AccessTools.Method(typeof(SettlementUpgrades), nameof(CreateProductPostfix))));
            _harmony.Patch(
                AccessTools.Method(typeof(Recipe), nameof(Recipe.UseIngredients),
                    new[]
                    {
                        typeof(Character), typeof(Equipment), typeof(List<UsedIngredient>),
                        typeof(float).MakeByRefType(), typeof(InfectionType).MakeByRefType(),
                        typeof(bool).MakeByRefType(), typeof(bool),
                    }),
                prefix: new HarmonyMethod(AccessTools.Method(typeof(SettlementUpgrades), nameof(UseIngredientsPrefix))));
            // Watch: the guard occupying an upgraded tower sees
            // farther. Postfix the out-param overload
            // (GetSightRange has a no-arg sibling).
            _harmony.Patch(
                AccessTools.Method(typeof(Character), nameof(Character.GetSightRange),
                    new[] { typeof(int).MakeByRefType(), typeof(int).MakeByRefType() }),
                postfix: new HarmonyMethod(AccessTools.Method(typeof(SettlementUpgrades), nameof(GetSightRangePostfix))));
            // Yield (settlement-wide): the owning camp's Yield level
            // lifts every crop's max yield.
            _harmony.Patch(
                AccessTools.Method(typeof(PlantableCrop), nameof(PlantableCrop.GetMaxYield)),
                postfix: new HarmonyMethod(AccessTools.Method(typeof(SettlementUpgrades), nameof(GetMaxYieldPostfix))));
            _installed = true;
            ShimLogger.Info("SettlementUpgrades: installed (effects + upgrade menu patches, 10 per-structure + 1 settlement-wide tracks)");
        }
        catch (Exception e)
        {
            ShimLogger.Error("SettlementUpgrades: install FAILED: " + e);
        }
    }

    // ---- the menu ---------------------------------------------------------------

    /// Which structure is upgradeable by the controlled character:
    /// the player's own, fully built, destructible, with a repair
    /// resource to price the work in.
    private static bool Upgradeable(Prop prop, Character character)
    {
        if (prop == null || character == null) return false;
        if (prop.Destroyed || prop.UnderConstructionInfo != null) return false;
        var proto = prop.GetPropPrototype();
        if (proto == null || !(proto.MaxDamage > 0f)) return false;
        if (prop.GetRepairResourceType() == null) return false;
        var com = character.Community;
        return com != null && prop.GetCommunity() == com;
    }

    private static int CostFor(PropPrototype proto, int nextLevel)
    {
        var baseNeed = Math.Max(1, (int)Math.Ceiling(proto.RepairResourceNeeded));
        return baseNeed * CostFactor * nextLevel;
    }

    private static int SkillFor(PropPrototype proto, int nextLevel)
    {
        return proto.RepairSkillNeeded + (nextLevel - 1) / LevelsPerSkillBand;
    }

    private static int CountCarried(Character c, EquipmentPrototype proto)
    {
        var total = 0;
        foreach (var item in c.Inventory.Contents)
        {
            if (item.GetPrototype() == proto) total += item.GetAmount();
        }
        return total;
    }

    /// Append the upgrade entries for the hovered structure: one
    /// per track that fits what the building does.
    private static void GetAvailableActionsPostfix(GameCursor __instance, ref BaseObject outTarget)
    {
        try
        {
            var prop = outTarget as Prop;
            var character = Hud.Instance?.LocalControlledCharacter;
            if (!Upgradeable(prop, character)) return;
            var proto = prop.GetPropPrototype();
            var resource = prop.GetRepairResourceType();
            var have = CountCarried(character, resource);
            var skill = character.GetSkillLevelWithEffects(SkillType.Construction);
            for (var i = 0; i < TrackDefs.Length; i++)
            {
                if (!TrackDefs[i].Applies(prop)) continue;
                var next = GetLevel(prop.Id, TrackDefs[i].Name) + 1;
                var cost = CostFor(proto, next);
                var reason = skill < SkillFor(proto, next)
                    ? CursorActionDisabledReason.ConstructionSkillTooLow
                    : CursorActionDisabledReason.Enabled;
                var label = TrackDefs[i].Name + " +" + next + ": " + cost + " " + resource.NativeName;
                if (have < cost) label += " (carrying " + have + ")";
                var action = new AvailableAction(
                    (CursorAction)(SentinelBase + i), character, prop, reason)
                {
                    SpeechText = label,
                };
                __instance.AvailableActions.Add(action);
            }
            // Settlement-wide tracks live on the Command Post and are
            // keyed by the camp, not this structure.
            if (IsCommandPost(prop))
            {
                var com = prop.GetCommunity();
                var comId = com == null ? 0 : com.Id;
                for (var i = 0; i < SettlementTracks.Length; i++)
                {
                    var next = GetCommunityLevel(comId, SettlementTracks[i]) + 1;
                    var cost = CostFor(proto, next);
                    var reason = skill < SkillFor(proto, next)
                        ? CursorActionDisabledReason.ConstructionSkillTooLow
                        : CursorActionDisabledReason.Enabled;
                    var label = SettlementTracks[i] + " (camp) +" + next + ": " + cost + " " + resource.NativeName;
                    if (have < cost) label += " (carrying " + have + ")";
                    var action = new AvailableAction(
                        (CursorAction)(SettlementSentinelBase + i), character, prop, reason)
                    {
                        SpeechText = label,
                    };
                    __instance.AvailableActions.Add(action);
                }
            }
        }
        catch (Exception e)
        {
            ShimLogger.Warn("SettlementUpgrades: menu population failed: " + e.Message);
        }
    }

    /// Our entries carry their label in SpeechText; vanilla
    /// captions index arrays by action id, which a sentinel must
    /// never reach.
    private static bool GetCaptionPrefix(ref AvailableAction __instance, ref string __result)
    {
        if ((int)__instance.ActionType < SentinelBase) return true;
        __result = __instance.SpeechText;
        return false;
    }

    /// The click: consume the materials for real, bump the track,
    /// tell the player. Skips the vanilla switch for our ids.
    private static bool OnSelectedActionPrefix(AvailableAction action)
    {
        if ((int)action.ActionType < SentinelBase) return true;
        try
        {
            HandleUpgradeClick(action);
        }
        catch (Exception e)
        {
            ShimLogger.Warn("SettlementUpgrades: upgrade click failed: " + e);
        }
        return false;
    }

    private static void HandleUpgradeClick(AvailableAction action)
    {
        if ((int)action.ActionType >= SettlementSentinelBase)
        {
            HandleSettlementUpgradeClick(action);
            return;
        }
        var trackIx = (int)action.ActionType - SentinelBase;
        if (trackIx < 0 || trackIx >= TrackDefs.Length) return;
        var track = TrackDefs[trackIx].Name;
        var prop = action.Target as Prop;
        var character = action.Actor;
        if (!Upgradeable(prop, character)) return;
        if (action.Enabled != CursorActionDisabledReason.Enabled) return;
        var proto = prop.GetPropPrototype();
        var resource = prop.GetRepairResourceType();
        var next = GetLevel(prop.Id, track) + 1;
        var cost = CostFor(proto, next);
        var have = CountCarried(character, resource);
        if (have < cost)
        {
            HudBehaviour.Instance?.SetStatusBarMsg(
                "Upgrade needs " + cost + " " + resource.NativeName + " (carrying " + have + ")");
            return;
        }
        // Real consumption from the character's carried stacks.
        var remaining = cost;
        while (remaining > 0)
        {
            var item = character.Inventory.FindItemOfType(resource);
            if (item == null) break; // counted above; belt and braces
            var take = Math.Min(remaining, item.GetAmount());
            var taken = character.Inventory.Take(character, item, take);
            taken?.Delete();
            remaining -= take;
        }
        SetLevel(prop.Id, track, next);
        HudBehaviour.Instance?.SetStatusBarMsg(
            prop.GetDisplayNameString() + ": " + track + " +" + next
            + " (" + cost + " " + resource.NativeName + " used)");
        ShimLogger.Info("SettlementUpgrades: " + proto.Name + " #" + prop.Id
            + " " + track + " -> " + next + " (" + cost + " " + resource.Name + " consumed)");
    }

    /// The Command Post click for a settlement-wide track: same
    /// real material cost from the character's carried stacks, but
    /// the level is stored on the CAMP (the prop's community), so
    /// the effect covers everything that community owns.
    private static void HandleSettlementUpgradeClick(AvailableAction action)
    {
        var ix = (int)action.ActionType - SettlementSentinelBase;
        if (ix < 0 || ix >= SettlementTracks.Length) return;
        var track = SettlementTracks[ix];
        var prop = action.Target as Prop;
        var character = action.Actor;
        if (!Upgradeable(prop, character) || !IsCommandPost(prop)) return;
        if (action.Enabled != CursorActionDisabledReason.Enabled) return;
        var com = prop.GetCommunity();
        if (com == null) return;
        var proto = prop.GetPropPrototype();
        var resource = prop.GetRepairResourceType();
        var next = GetCommunityLevel(com.Id, track) + 1;
        var cost = CostFor(proto, next);
        var have = CountCarried(character, resource);
        if (have < cost)
        {
            HudBehaviour.Instance?.SetStatusBarMsg(
                "Upgrade needs " + cost + " " + resource.NativeName + " (carrying " + have + ")");
            return;
        }
        var remaining = cost;
        while (remaining > 0)
        {
            var item = character.Inventory.FindItemOfType(resource);
            if (item == null) break;
            var take = Math.Min(remaining, item.GetAmount());
            var taken = character.Inventory.Take(character, item, take);
            taken?.Delete();
            remaining -= take;
        }
        SetCommunityLevel(com.Id, track, next);
        HudBehaviour.Instance?.SetStatusBarMsg(
            "Settlement: " + track + " +" + next
            + " (" + cost + " " + resource.NativeName + " used)");
        ShimLogger.Info("SettlementUpgrades: community #" + com.Id
            + " " + track + " -> " + next + " (" + cost + " " + resource.Name + " consumed)");
    }

    // ---- the track effects -------------------------------------------------------

    /// Shared diminishing curve for multiplier tracks.
    private static float CurveBonus(int level, float baseStep, float decay)
    {
        float bonus = 0f, step = baseStep;
        for (var i = 0; i < level; i++)
        {
            bonus += step;
            step *= decay;
        }
        return bonus;
    }

    /// Expand: storage capacity rides the track.
    private static void GetMaxInventoryWeightPostfix(Prop __instance, ref float __result)
    {
        if (!(__result > 0f)) return;
        var level = GetLevel(__instance.Id, TrackExpand);
        if (level <= 0) return;
        __result *= 1f + CurveBonus(level, 0.5f, 0.85f);
    }

    /// Spikes: a melee attacker bleeds on the structure it hits.
    private static void ApplyDamagePostfix(Prop __instance, Character source, bool burning, float damageRadius)
    {
        try
        {
            if (source == null || burning || damageRadius > 0f) return;
            var level = Math.Min(10, GetLevel(__instance.Id, TrackSpikes));
            if (level <= 0) return;
            if (!source.AliveAndNotZombie && !source.Zombie) return;
            // Melee reach only: explosions and gunfire pass wider
            // radii or no adjacency.
            if ((source.Pos - __instance.Pos).sqrMagnitude > 16f) return;
            var bone = UnityEngine.Random.value < 0.5f ? Bone.LeftLeg : Bone.RightLeg;
            source.OnMeleeAttack(AttackType.Low, InjuryType.SharpObject, bone, SpikeDamagePerLevel * level);
        }
        catch (Exception e)
        {
            ShimLogger.Warn("SettlementUpgrades: spikes failed: " + e.Message);
        }
    }

    /// Speed: crafting at the prop runs faster.
    private static void CraftPostfix(CraftingProp __instance, float time)
    {
        var level = GetLevel(__instance.Id, TrackSpeed);
        if (level <= 0) return;
        __instance.CraftingTimeSpent += time * CurveBonus(level, SpeedBase, SpeedDecay);
    }

    /// Productivity: a chance per craft of one extra product,
    /// spawned into the same carrier (the prop for prop-crafts).
    private static void CreateProductPostfix(Recipe __instance, TileObject carrier, bool __result)
    {
        try
        {
            if (!__result || __instance.ProductPrototype == null) return;
            if (!(carrier is Prop prop)) return;
            var level = GetLevel(prop.Id, TrackProductivity);
            if (level <= 0) return;
            var chance = Math.Min(CraftChanceCap, ProductivityChancePerLevel * level);
            if (UnityEngine.Random.value >= chance) return;
            var extra = Equipment.Spawn(__instance.ProductPrototype, Math.Max(1, __instance.ProductAmount));
            if (extra == null) return;
            prop.Inventory.Add(prop, extra);
            ShimLogger.Info("SettlementUpgrades: productivity bonus at " + prop.GetPropPrototype().Name
                + " #" + prop.Id + ": extra " + __instance.ProductPrototype.Name);
        }
        catch (Exception e)
        {
            ShimLogger.Warn("SettlementUpgrades: productivity failed: " + e.Message);
        }
    }

    /// Quality bonus handoff: recorded when ingredients are used
    /// near an upgraded work prop, consumed by the Rust craft
    /// roll seconds later.
    private static readonly Dictionary<int, int> CraftQualityBonus = new Dictionary<int, int>();

    public static int TakeCraftQualityBonus(int characterId)
    {
        if (CraftQualityBonus.TryGetValue(characterId, out var level))
        {
            CraftQualityBonus.Remove(characterId);
            return level;
        }
        return 0;
    }

    /// The work prop this recipe would run on, near the carrier.
    private static CraftingProp NearestWorkProp(Recipe recipe, Character carrier)
    {
        var session = Session.Instance;
        if (session == null || carrier == null) return null;
        CraftingProp best = null;
        var bestD = 25f; // within 5m
        foreach (var obj in session.PropManager.AllProps)
        {
            if (!(obj is CraftingProp cp)) continue;
            if (!recipe.IsCraftingPropForRecipe(cp)) continue;
            var d = (cp.Pos - carrier.Pos).sqrMagnitude;
            if (d < bestD)
            {
                bestD = d;
                best = cp;
            }
        }
        return best;
    }

    /// Efficiency: a chance the craft consumes nothing. Also the
    /// moment the Quality handoff is recorded (same lookup).
    private static bool UseIngredientsPrefix(
        Recipe __instance,
        Character carrier,
        ref float ingredientsNutrition,
        ref InfectionType ingredientsInfectedWith,
        ref bool usedHumanIngredients)
    {
        ingredientsNutrition = 0f;
        ingredientsInfectedWith = InfectionType.None;
        usedHumanIngredients = false;
        try
        {
            var prop = NearestWorkProp(__instance, carrier);
            if (prop == null) return true;
            var quality = GetLevel(prop.Id, TrackQuality);
            if (quality > 0 && carrier != null)
            {
                CraftQualityBonus[carrier.Id] = quality;
            }
            var level = GetLevel(prop.Id, TrackEfficiency);
            if (level <= 0) return true;
            var chance = Math.Min(CraftChanceCap, EfficiencyChancePerLevel * level);
            if (UnityEngine.Random.value >= chance) return true;
            ShimLogger.Info("SettlementUpgrades: efficiency bonus at " + prop.GetPropPrototype().Name
                + " #" + prop.Id + ": ingredients refunded");
            return false; // skip consumption; the materials stay
        }
        catch (Exception e)
        {
            ShimLogger.Warn("SettlementUpgrades: efficiency failed: " + e.Message);
            return true;
        }
    }

    /// Watch: the guard occupying an upgraded tower sees farther.
    /// The game already adds the occupied building's slot modifier
    /// to sight range (Character.GetSightRange); this rides the same
    /// path off the tower's track level. fogStart is the sight
    /// range in tiles; the game caps it at 31 (re-clamped here so
    /// the bonus stops there too).
    private static void GetSightRangePostfix(Character __instance, ref int fogStart, ref int fogEnd)
    {
        try
        {
            if (__instance == null || __instance.Zombie) return;
            var building = __instance.InsideBuilding;
            if (building == null) return;
            var level = GetLevel(building.Id, TrackWatch);
            if (level <= 0) return;
            fogStart = Math.Min(SightRangeCap, fogStart + WatchTilesPerLevel * level);
            fogEnd = Math.Min(SightRangeCap, fogStart + 8);
        }
        catch (Exception e)
        {
            ShimLogger.Warn("SettlementUpgrades: watch failed: " + e.Message);
        }
    }

    /// Yield (settlement-wide): the owning camp's Yield level lifts
    /// every crop's max yield, which feeds the harvest amount
    /// (PlantableCrop sets HarvestableAmount from GetMaxYield). Keyed
    /// by the crop's community, so it covers the whole camp's fields.
    private static void GetMaxYieldPostfix(PlantableCrop __instance, ref int __result)
    {
        try
        {
            if (__result <= 0) return;
            var com = __instance.GetCommunity();
            if (com == null) return;
            var level = GetCommunityLevel(com.Id, TrackYield);
            if (level <= 0) return;
            __result = UnityEngine.Mathf.CeilToInt(__result * (1f + CurveBonus(level, YieldBase, YieldDecay)));
        }
        catch (Exception e)
        {
            ShimLogger.Warn("SettlementUpgrades: yield failed: " + e.Message);
        }
    }

    /// Secure: a hostile taking (the mod's theft, predation, and
    /// tribute acts) tests the storage's locks before draining it.
    /// Queried per building from the Rust acts; one roll per visit.
    public static bool SecureBlocks(int propId)
    {
        var level = GetLevel(propId, TrackSecure);
        if (level <= 0) return false;
        var chance = Math.Min(SecureBlockCap, SecureBlockChancePerLevel * level);
        if (UnityEngine.Random.value >= chance) return false;
        ShimLogger.Info("SettlementUpgrades: Secure held (prop #" + propId
            + ", level " + level + ")");
        return true;
    }

    // ---- health regen (driver tick) ------------------------------------------------

    private static float _lastRegen;

    /// Called every frame from the shim driver; heals tracked
    /// structures on a slow cadence. Damage is a public
    /// per-instance field, so no patch is needed.
    public static void Tick(float now)
    {
        if (now - _lastRegen < RegenTickSecs) return;
        var dt = now - _lastRegen;
        _lastRegen = now;
        try
        {
            var session = Session.Instance;
            if (session == null || !_loaded) return;
            var any = false;
            foreach (var p in Tracks)
            {
                if (p.Value.TryGetValue(TrackHealthRegen, out var l) && l > 0)
                {
                    any = true;
                    break;
                }
            }
            if (!any) return;
            foreach (var obj in session.PropManager.AllProps)
            {
                if (!(obj is Prop prop) || prop.Destroyed) continue;
                var frac = prop.GetDamageFraction();
                if (frac <= 0f) continue;
                var level = GetLevel(prop.Id, TrackHealthRegen);
                if (level <= 0) continue;
                var maxDamage = prop.GetMaxDamage();
                if (maxDamage <= 0f || maxDamage >= float.MaxValue) continue;
                var heal = RegenHpPerMinPerLevel * level * (dt / 60f);
                prop.SetDamageFraction(Math.Max(0f, frac - heal / maxDamage));
            }
        }
        catch (Exception e)
        {
            ShimLogger.Warn("SettlementUpgrades: regen tick failed: " + e.Message);
        }
    }

    // ---- the Reinforce effect ------------------------------------------------

    private static void GetMaxDamagePostfix(Prop __instance, ref float __result)
    {
        // Indestructible stays indestructible.
        if (__result >= float.MaxValue) return;
        var level = GetLevel(__instance.Id, TrackReinforce);
        if (level <= 0) return;
        __result *= 1f + ReinforceBonus(level);
    }

    public static float ReinforceBonus(int level)
    {
        float bonus = 0f, step = ReinforceBase;
        for (var i = 0; i < level; i++)
        {
            bonus += step;
            step *= ReinforceDecay;
        }
        return bonus;
    }

    // ---- the sidecar store ----------------------------------------------------

    private static string StorePath(long seed)
    {
        var profile = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        return Path.Combine(profile,
            "AppData", "LocalLow", "Ginormocorp Industries", "Survivalist Invisible Strain",
            "survivalist-mod.upgrades.seed" + seed + ".json");
    }

    /// Lazy per-save load; reloads when the seed changes (a
    /// different save was loaded). Returns false at the menu.
    private static bool EnsureLoaded()
    {
        var session = Session.Instance;
        if (session == null) return false;
        long seed = session.RandomSeed;
        if (_loaded && seed == _seed) return true;
        Tracks.Clear();
        CommunityTracks.Clear();
        _seed = seed;
        _loaded = true;
        try
        {
            var path = StorePath(seed);
            if (File.Exists(path))
            {
                var root = JObject.Parse(File.ReadAllText(path));
                if (root["props"] is JObject props)
                {
                    foreach (var p in props)
                    {
                        if (!(p.Value is JObject trackObj)) continue;
                        var levels = new Dictionary<string, int>();
                        foreach (var t in trackObj) levels[t.Key] = (int)t.Value;
                        Tracks[int.Parse(p.Key)] = levels;
                    }
                }
                if (root["communities"] is JObject coms)
                {
                    foreach (var c in coms)
                    {
                        if (!(c.Value is JObject trackObj)) continue;
                        var levels = new Dictionary<string, int>();
                        foreach (var t in trackObj) levels[t.Key] = (int)t.Value;
                        CommunityTracks[int.Parse(c.Key)] = levels;
                    }
                }
                ShimLogger.Info("SettlementUpgrades: restored upgrades for "
                    + Tracks.Count + " structure(s) and " + CommunityTracks.Count
                    + " camp(s) (seed " + seed + ")");
            }
        }
        catch (Exception e)
        {
            ShimLogger.Warn("SettlementUpgrades: sidecar load failed: " + e.Message);
        }
        return true;
    }

    /// Atomic tmp-then-rename write (the genome store's shape).
    private static void Persist()
    {
        if (!_loaded) return;
        try
        {
            var props = new JObject();
            foreach (var p in Tracks)
            {
                var trackObj = new JObject();
                foreach (var t in p.Value) trackObj[t.Key] = t.Value;
                props[p.Key.ToString()] = trackObj;
            }
            var coms = new JObject();
            foreach (var c in CommunityTracks)
            {
                var trackObj = new JObject();
                foreach (var t in c.Value) trackObj[t.Key] = t.Value;
                coms[c.Key.ToString()] = trackObj;
            }
            var root = new JObject
            {
                ["schema_version"] = 1,
                ["props"] = props,
                ["communities"] = coms,
            };
            var path = StorePath(_seed);
            var tmp = path + ".tmp";
            File.WriteAllText(tmp, root.ToString(Newtonsoft.Json.Formatting.None));
            if (File.Exists(path)) File.Delete(path);
            File.Move(tmp, path);
        }
        catch (Exception e)
        {
            ShimLogger.Warn("SettlementUpgrades: sidecar write failed: " + e.Message);
        }
    }

    public static int GetLevel(int propId, string track)
    {
        if (!_loaded && !EnsureLoaded()) return 0;
        return Tracks.TryGetValue(propId, out var t) && t.TryGetValue(track, out var level)
            ? level
            : 0;
    }

    public static void SetLevel(int propId, string track, int level)
    {
        if (!EnsureLoaded()) return;
        if (!Tracks.TryGetValue(propId, out var t))
        {
            t = new Dictionary<string, int>();
            Tracks[propId] = t;
        }
        t[track] = level;
        Persist();
    }

    public static int GetCommunityLevel(int communityId, string track)
    {
        if (!_loaded && !EnsureLoaded()) return 0;
        return CommunityTracks.TryGetValue(communityId, out var t) && t.TryGetValue(track, out var level)
            ? level
            : 0;
    }

    public static void SetCommunityLevel(int communityId, string track, int level)
    {
        if (!EnsureLoaded()) return;
        if (!CommunityTracks.TryGetValue(communityId, out var t))
        {
            t = new Dictionary<string, int>();
            CommunityTracks[communityId] = t;
        }
        t[track] = level;
        Persist();
    }

    // ---- probes (driven by the Rust ops; permanent diagnostics) ---------------

    /// Task 1 gate: set Reinforce on the player camp's first
    /// structure of the named type and read the hit points back
    /// through the game's own getter. Returns a JSON report.
    public static string UpgradeProbe(string propTypeName, int level)
    {
        var session = Session.Instance;
        if (session == null) return Err("no session (menu?)");
        Community player = null;
        foreach (var com in session.CommunityManager.Communities)
        {
            if (com.CommunityType == CommunityType.Player) { player = com; break; }
        }
        if (player == null) return Err("no player community");
        Prop found = null;
        foreach (var obj in session.PropManager.AllProps)
        {
            if (obj is Prop prop
                && prop.GetPropPrototype() != null
                && prop.GetPropPrototype().Name == propTypeName
                && prop.GetCommunity() == player)
            {
                found = prop;
                break;
            }
        }
        if (found == null) return Err("player has no '" + propTypeName + "'");
        var before = found.GetMaxDamage();
        SetLevel(found.Id, TrackReinforce, level);
        var after = found.GetMaxDamage();
        var report = new JObject
        {
            ["id"] = found.Id,
            ["type"] = propTypeName,
            ["level"] = level,
            ["bonus"] = ReinforceBonus(level),
            ["max_hp_before"] = before,
            ["max_hp_after"] = after,
        };
        ShimLogger.Info("SettlementUpgrades: probe set " + propTypeName + " #" + found.Id
            + " Reinforce=" + level + " (hp " + before + " -> " + after + ")");
        return report.ToString(Newtonsoft.Json.Formatting.None);
    }

    /// The upgrade_status op's data: totals per track.
    public static string Status()
    {
        if (!EnsureLoaded()) return Err("no session (menu?)");
        var perTrack = new Dictionary<string, int>();
        var levels = 0;
        foreach (var p in Tracks)
        {
            foreach (var t in p.Value)
            {
                perTrack.TryGetValue(t.Key, out var n);
                perTrack[t.Key] = n + t.Value;
                levels += t.Value;
            }
        }
        var tracks = new JObject();
        foreach (var t in perTrack) tracks[t.Key] = t.Value;
        var root = new JObject
        {
            ["structures_upgraded"] = Tracks.Count,
            ["levels_total"] = levels,
            ["levels_per_track"] = tracks,
            ["seed"] = _seed,
        };
        return root.ToString(Newtonsoft.Json.Formatting.None);
    }

    private static string Err(string msg)
    {
        return new JObject { ["error"] = msg }.ToString(Newtonsoft.Json.Formatting.None);
    }
}
