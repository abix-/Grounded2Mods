// Plugin.cs (IL2CPP). BepInEx 6 IL2CPP entry. Wires the host
// log, Harmony, and per-frame driver to the shared
// GenerationLoader. IL2CPP class injection and backend access
// remain specific to this shim.

using System;
using System.IO;
using System.Reflection;
using BepInEx.Unity.IL2CPP;
using Il2CppInterop.Runtime.Injection;
using UnityEngine;

namespace Unityforge.Shim
{
    [BepInEx.BepInPlugin(PluginGuid, PluginName, PluginVersion)]
    public class UnityforgeShimIl2CppPlugin : BasePlugin
    {
        public const string PluginGuid = "abix.unityforge.shim.il2cpp";
        public const string PluginName = "Unityforge.Shim.Il2Cpp";
        public const string PluginVersion = "0.1.0";

        private GenerationLoader _loader;
        private UnityforgeTickDriver _driver;

        public override void Load()
        {
            var src = base.Log;
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
            ShimLogger.Info("Unityforge.Shim.Il2Cpp: Load");

            var dir = Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location);
            var dllPath = GenerationLoader.LocateRustDll(dir);
            if (dllPath == null)
            {
                ShimLogger.Error("Unityforge.Shim.Il2Cpp: no Rust target DLL found. Set "
                    + GenerationLoader.TargetEnv
                    + " or drop a *.unityforge.dll next to this plugin.");
                return;
            }

            HarmonyBridge.AcquireHandle = Il2CppBridge.Acquire;
            HarmonyBridge.EnsureHarmony(PluginGuid);

            _loader = new GenerationLoader(new Il2CppBackendBridge(), Il2CppBridge.ClearHandles);
            if (!_loader.LoadInitial(dllPath))
            {
                ShimLogger.Error("Unityforge.Shim.Il2Cpp: initial generation failed to load");
                _loader = null;
                return;
            }

            // IL2CPP MonoBehaviours must be registered with the
            // Il2Cpp class injector before AddComponent.
            ClassInjector.RegisterTypeInIl2Cpp<UnityforgeTickDriver>();
            var go = new GameObject("UnityforgeTickDriver");
            UnityEngine.Object.DontDestroyOnLoad(go);
            _driver = go.AddComponent<UnityforgeTickDriver>();
            _driver.Tick = _loader.Tick;

            ShimLogger.Info("Unityforge.Shim.Il2Cpp: ready (generation 0)");
        }

        public override bool Unload()
        {
            if (_driver != null)
            {
                _driver.Tick = null;
                if (_driver.gameObject != null)
                    UnityEngine.Object.Destroy(_driver.gameObject);
                _driver = null;
            }
            _loader?.ShutdownFinal();
            _loader = null;
            return true;
        }
    }

    /// <summary>
    /// IL2CPP-injected MonoBehaviour that drives unityforge_tick
    /// every frame. Held alive by the GameObject created at Load.
    /// </summary>
    public class UnityforgeTickDriver : MonoBehaviour
    {
        public UnityforgeTickDriver(IntPtr ptr) : base(ptr) { }

        // Managed delegate retained by the injected driver. The
        // shared loader owns generation selection and dispatch.
        public Action<float> Tick;

        private void Update()
        {
            if (Tick == null) return;
            InputBridge.PollAll();
            try { Tick(Time.realtimeSinceStartup); }
            catch (Exception e) { ShimLogger.Error("Unityforge.Shim.Il2Cpp: tick threw: " + e); }
        }
    }
}
