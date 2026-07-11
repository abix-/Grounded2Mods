// Main.cs. Survivalist: Invisible Strain official-loader host.
//
// The game's Story.LoadDLLs() loads every *.dll in a story/mod
// folder's DLLs directory, looks up the GLOBAL-namespace type
// "Main", and invokes `public static void Load()`; on story
// content unload Story.UnloadDLLs() invokes `Unload()`. Both run
// on the Unity main thread. Managed assemblies never leave the
// process, so a later story select calls Load() again on the SAME
// statics; the GenerationLoader's park + re-init path handles
// that.
//
// The Rust cdylib (*.unityforge.dll) sits in the MOD FOLDER, one
// level above DLLs/, so the game's loader doesn't try to
// Assembly.LoadFrom a native image (it would log a
// BadImageFormat error on every story load).

using System.IO;
using System.Reflection;
using UnityEngine;
using Unityforge.Shim;

public class Main
{
    private static GenerationLoader _loader;
    private static GameObject _driverGo;

    public static void Load()
    {
        ShimLogger.Sink = (level, msg) =>
        {
            switch (level)
            {
                case 3: Debug.LogWarning("[Unityforge] " + msg); break;
                case 4: Debug.LogError("[Unityforge] " + msg); break;
                default: Debug.Log("[Unityforge] " + msg); break;
            }
        };

        // Re-entry: a prior Unload() parked the generation with
        // its module still mapped; re-arm it instead of loading
        // a second image.
        if (_loader != null)
        {
            if (_loader.ReinitAfterUnload())
            {
                SettlementUpgrades.Install();
                EnsureDriver();
                ShimLogger.Info("Unityforge.Shim: re-armed after unload");
            }
            else
            {
                ShimLogger.Error("Unityforge.Shim: re-init after unload failed");
            }
            return;
        }

        ShimLogger.Info("Unityforge.Shim: Load (Survivalist official loader)");

        var shimDir = Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location);
        var modDir = Path.GetDirectoryName(shimDir);
        var dllPath = GenerationLoader.LocateRustDll(modDir);
        if (dllPath == null)
        {
            ShimLogger.Error("Unityforge.Shim: no Rust target DLL found. Set "
                + GenerationLoader.TargetEnv
                + " or drop exactly one *.unityforge.dll in the mod folder (next to DLLs/): " + modDir);
            return;
        }

        HarmonyBridge.EnsureHarmony("abix.unityforge.shim.survivalist");

        // Settlement upgrades: game-typed Harmony patches
        // (Upgrades.cs). Idempotent; patches survive story
        // switches with the assembly.
        SettlementUpgrades.Install();

        var loader = new GenerationLoader(new MonoBackendBridge(), MonoBridge.ClearHandles);
        if (!loader.LoadInitial(dllPath))
        {
            ShimLogger.Error("Unityforge.Shim: initial generation failed to load");
            return;
        }
        _loader = loader;
        EnsureDriver();
        ShimLogger.Info("Unityforge.Shim: ready (generation 0)");
    }

    public static void Unload()
    {
        if (_driverGo != null)
        {
            Object.Destroy(_driverGo);
            _driverGo = null;
        }
        _loader?.ShutdownForUnload();
        ShimLogger.Info("Unityforge.Shim: Unload complete");
    }

    internal static void DriverUpdate()
    {
        var loader = _loader;
        if (loader == null || !loader.Active) return;
        InputBridge.PollAll();
        SettlementUpgrades.Tick(Time.realtimeSinceStartup);
        loader.Tick(Time.realtimeSinceStartup);
    }

    private static void EnsureDriver()
    {
        if (_driverGo != null) return;
        _driverGo = new GameObject("Unityforge.SurvivalistDriver");
        Object.DontDestroyOnLoad(_driverGo);
        _driverGo.AddComponent<SurvivalistDriver>();
    }
}

/// <summary>
/// Persistent MonoBehaviour that drives the per-frame tick
/// (input poll + hot-reload check + unityforge_tick). Created by
/// Main.Load, destroyed by Main.Unload.
/// </summary>
public class SurvivalistDriver : MonoBehaviour
{
    private void Update()
    {
        Main.DriverUpdate();
    }
}
