// Mod.cs. MelonLoader entry for the IL2CPP backend. Same seam as
// the BepInEx entries: wires the log sink to MelonLogger, locates
// the Rust cdylib next to this DLL in the game's Mods folder, and
// drives the shared GenerationLoader (cs-shim-common) from
// OnUpdate. Generation loading + hot reload live in
// GenerationLoader; the Il2Cpp reflection + Harmony backend is
// the same Il2CppBridge the BepInEx 6 shim uses. MelonLoader
// ships HarmonyX (2.10.2 in 0.7.2) and Il2CppInterop, so the
// shared HarmonyBridge binds unchanged.
//
// First target: Schedule 1 (IL2CPP default branch). The shim is
// game-agnostic; MelonGame is left universal.

using System.IO;
using System.Reflection;
using MelonLoader;
using UnityEngine;
using Unityforge.Shim;

[assembly: MelonInfo(typeof(Unityforge.Shim.UnityforgeShimMelonMod), "Unityforge.Shim.Melon", "0.1.0", "abix")]
[assembly: MelonGame]

namespace Unityforge.Shim
{
    public class UnityforgeShimMelonMod : MelonMod
    {
        private GenerationLoader _loader;

        public override void OnInitializeMelon()
        {
            var log = LoggerInstance;
            ShimLogger.Sink = (level, msg) =>
            {
                switch (level)
                {
                    case 0:
                    case 1:
                    case 2: log.Msg(msg); break;
                    case 3: log.Warning(msg); break;
                    case 4: log.Error(msg); break;
                    default: log.Msg(msg); break;
                }
            };
            ShimLogger.Info("Unityforge.Shim.Melon: init");

            var dir = Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location);
            var dllPath = GenerationLoader.LocateRustDll(dir);
            if (dllPath == null)
            {
                ShimLogger.Error("Unityforge.Shim.Melon: no Rust target DLL found. Set "
                    + GenerationLoader.TargetEnv
                    + " or drop a *.unityforge.dll next to this mod.");
                return;
            }

            HarmonyBridge.AcquireHandle = Il2CppBridge.Acquire;
            HarmonyBridge.EnsureHarmony("abix.unityforge.shim.melon");

            _loader = new GenerationLoader(new Il2CppBackendBridge(), Il2CppBridge.ClearHandles);
            if (!_loader.LoadInitial(dllPath))
            {
                ShimLogger.Error("Unityforge.Shim.Melon: initial generation failed to load");
                _loader = null;
                return;
            }
            ShimLogger.Info("Unityforge.Shim.Melon: ready (generation 0)");
        }

        public override void OnUpdate()
        {
            if (_loader == null || !_loader.Active) return;
            InputBridge.PollAll();
            _loader.Tick(Time.realtimeSinceStartup);
        }

        public override void OnApplicationQuit()
        {
            _loader?.ShutdownFinal();
            _loader = null;
        }
    }
}
