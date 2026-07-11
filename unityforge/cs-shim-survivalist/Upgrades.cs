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

    // Sentinel menu action ids, far above the vanilla enum range
    // (the game's switches ignore unknown values; our caption
    // prefix intercepts before any array indexes by action).
    private const int SentinelBase = 9000;
    private const CursorAction ReinforceAction = (CursorAction)(SentinelBase + 0);

    private static Harmony _harmony;
    private static bool _installed;

    // prop id -> track -> level. Loaded per save seed.
    private static readonly Dictionary<int, Dictionary<string, int>> Tracks
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
            _installed = true;
            ShimLogger.Info("SettlementUpgrades: installed (max-damage effect + upgrade menu patches)");
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

    /// Append the upgrade entries for the hovered structure.
    private static void GetAvailableActionsPostfix(GameCursor __instance, ref BaseObject outTarget)
    {
        try
        {
            var prop = outTarget as Prop;
            var character = Hud.Instance?.LocalControlledCharacter;
            if (!Upgradeable(prop, character)) return;
            var proto = prop.GetPropPrototype();
            var resource = prop.GetRepairResourceType();
            var next = GetLevel(prop.Id, TrackReinforce) + 1;
            var cost = CostFor(proto, next);
            var have = CountCarried(character, resource);
            var reason = CursorActionDisabledReason.Enabled;
            if (character.GetSkillLevelWithEffects(SkillType.Construction) < SkillFor(proto, next))
            {
                reason = CursorActionDisabledReason.ConstructionSkillTooLow;
            }
            var label = "Reinforce +" + next + ": " + cost + " " + resource.NativeName;
            if (have < cost) label += " (carrying " + have + ")";
            var action = new AvailableAction(ReinforceAction, character, prop, reason)
            {
                SpeechText = label,
            };
            __instance.AvailableActions.Add(action);
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
        var prop = action.Target as Prop;
        var character = action.Actor;
        if (!Upgradeable(prop, character)) return;
        if (action.Enabled != CursorActionDisabledReason.Enabled) return;
        var proto = prop.GetPropPrototype();
        var resource = prop.GetRepairResourceType();
        var next = GetLevel(prop.Id, TrackReinforce) + 1;
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
        SetLevel(prop.Id, TrackReinforce, next);
        var hp = prop.GetMaxDamage();
        HudBehaviour.Instance?.SetStatusBarMsg(
            prop.GetDisplayNameString() + " reinforced to +" + next
            + " (" + cost + " " + resource.NativeName + " used; "
            + hp.ToString("0.#") + " hp)");
        ShimLogger.Info("SettlementUpgrades: " + proto.Name + " #" + prop.Id
            + " Reinforce -> " + next + " (" + cost + " " + resource.Name + " consumed)");
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
                ShimLogger.Info("SettlementUpgrades: restored upgrades for "
                    + Tracks.Count + " structure(s) (seed " + seed + ")");
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
            var root = new JObject { ["schema_version"] = 1, ["props"] = props };
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
