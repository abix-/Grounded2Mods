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

    public const string TrackReinforce = "Reinforce";

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
            var original = AccessTools.Method(typeof(Prop), nameof(Prop.GetMaxDamage));
            var postfix = AccessTools.Method(typeof(SettlementUpgrades), nameof(GetMaxDamagePostfix));
            _harmony.Patch(original, postfix: new HarmonyMethod(postfix));
            _installed = true;
            ShimLogger.Info("SettlementUpgrades: installed (Prop.GetMaxDamage postfix)");
        }
        catch (Exception e)
        {
            ShimLogger.Error("SettlementUpgrades: install FAILED: " + e);
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
