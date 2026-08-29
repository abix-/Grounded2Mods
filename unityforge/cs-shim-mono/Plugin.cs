// Plugin.cs. BepInEx entry. Wires the log sink to BepInEx's
// ManualLogSource, locates the Rust cdylib next to this DLL, and
// drives the shared GenerationLoader (cs-shim-common) from a
// MonoBehaviour's Update. Generation loading + hot reload live in
// GenerationLoader; this file owns only the BepInEx host seam.
//
// See docs/unityforge-plan.md section 6.5 "Hot reload" for the
// generation-loader design + rationale.

using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using BepInEx;
using UnityEngine;

namespace Unityforge.Shim
{
    [BepInPlugin(PluginGuid, PluginName, PluginVersion)]
    public class UnityforgeShimPlugin : BaseUnityPlugin
    {
        public const string PluginGuid = "abix.unityforge.shim";
        public const string PluginName = "Unityforge.Shim";
        public const string PluginVersion = "0.1.0";

        private GenerationLoader _loader;

        private void Awake()
        {
            var src = base.Logger;
            ShimLogger.Sink = (level, msg) =>
            {
                switch (level)
                {
                    case 0:
                    case 1: src.LogDebug(msg); break;
                    case 2: src.LogInfo(msg); break;
                    case 3: src.LogWarning(msg); break;
                    case 4: src.LogError(msg); break;
                    default: src.LogInfo(msg); break;
                }
            };
            ShimLogger.Info("Unityforge.Shim: Awake");

            var dir = Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location);
            var dllPath = GenerationLoader.LocateRustDll(dir);
            if (dllPath == null)
            {
                ShimLogger.Error("Unityforge.Shim: no Rust target DLL found. Set " + GenerationLoader.TargetEnv + " or drop a *.unityforge.dll next to this plugin.");
                return;
            }

            HarmonyBridge.AcquireHandle = MonoBridge.Acquire;
            HarmonyBridge.EnsureHarmony(PluginGuid);

            _loader = new GenerationLoader(new MonoBackendBridge(), MonoBridge.ClearHandles);
            if (!_loader.LoadInitial(dllPath))
            {
                ShimLogger.Error("Unityforge.Shim: initial generation failed to load");
                _loader = null;
                return;
            }
            ShimLogger.Info("Unityforge.Shim: ready (generation 0)");
        }

        private void Update()
        {
            if (_loader == null || !_loader.Active) return;
            InputBridge.PollAll();
            _loader.Tick(Time.realtimeSinceStartup);
        }

        private void OnDestroy()
        {
            _loader?.ShutdownFinal();
            _loader = null;
        }
    }
}
