// Plugin.cs (IL2CPP). BepInEx 6 IL2CPP entry. Same shape as
// the Mono shim but uses `BasePlugin` from
// BepInEx.Unity.IL2CPP and adds a MonoBehaviour to drive the
// per-frame tick.

using System;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
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

        private const string TargetEnv = "UNITYFORGE_TARGET";

        private delegate int UnityforgeInitFn(IntPtr bridge);
        private delegate void UnityforgeTickFn(float now);
        private delegate void UnityforgeShutdownFn();

        private IntPtr _rustModule;
        private UnityforgeInitFn _init;
        private UnityforgeTickFn _tick;
        private UnityforgeShutdownFn _shutdown;
        private BridgeTable _bridge;
        private GCHandle _bridgeHandle;
        private bool _started;

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

            var dllPath = LocateRustDll();
            if (dllPath == null)
            {
                ShimLogger.Error("Unityforge.Shim.Il2Cpp: no Rust target DLL found. Set " + TargetEnv + " or drop a *.unityforge.dll next to this plugin.");
                return;
            }
            ShimLogger.Info("Unityforge.Shim.Il2Cpp: loading " + dllPath);

            _rustModule = NativeLibrary.Load(dllPath);
            if (_rustModule == IntPtr.Zero)
            {
                ShimLogger.Error("Unityforge.Shim.Il2Cpp: LoadLibrary failed: " + Marshal.GetLastWin32Error());
                return;
            }
            _init = ResolveSymbol<UnityforgeInitFn>("unityforge_init");
            _tick = ResolveSymbol<UnityforgeTickFn>("unityforge_tick");
            _shutdown = ResolveSymbol<UnityforgeShutdownFn>("unityforge_shutdown");
            if (_init == null || _tick == null || _shutdown == null)
            {
                ShimLogger.Error("Unityforge.Shim.Il2Cpp: target DLL is missing one of unityforge_init / unityforge_tick / unityforge_shutdown");
                return;
            }

            HarmonyBridge.EnsureHarmony(PluginGuid);

            _bridge = Bridge.Build(new Il2CppBackendBridge());
            _bridgeHandle = GCHandle.Alloc(_bridge, GCHandleType.Pinned);
            try
            {
                int rc = _init(_bridgeHandle.AddrOfPinnedObject());
                if (rc != 0)
                {
                    ShimLogger.Error("Unityforge.Shim.Il2Cpp: unityforge_init returned " + rc);
                    return;
                }
            }
            catch (Exception e)
            {
                ShimLogger.Error("Unityforge.Shim.Il2Cpp: unityforge_init threw: " + e);
                return;
            }

            // IL2CPP MonoBehaviours must be registered with the
            // Il2Cpp class injector before AddComponent.
            ClassInjector.RegisterTypeInIl2Cpp<UnityforgeTickDriver>();
            var go = new GameObject("UnityforgeTickDriver");
            UnityEngine.Object.DontDestroyOnLoad(go);
            var driver = go.AddComponent<UnityforgeTickDriver>();
            driver.Tick = _tick;

            _started = true;
            ShimLogger.Info("Unityforge.Shim.Il2Cpp: ready");
        }

        public override bool Unload()
        {
            if (_started)
            {
                try { _shutdown(); }
                catch (Exception e) { ShimLogger.Error("Unityforge.Shim.Il2Cpp: shutdown threw: " + e); }
            }
            if (_bridgeHandle.IsAllocated) _bridgeHandle.Free();
            if (_rustModule != IntPtr.Zero) NativeLibrary.Free(_rustModule);
            _started = false;
            return true;
        }

        private string LocateRustDll()
        {
            var explicitTarget = Environment.GetEnvironmentVariable(TargetEnv);
            if (!string.IsNullOrEmpty(explicitTarget) && File.Exists(explicitTarget))
                return explicitTarget;
            var dir = Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location);
            if (string.IsNullOrEmpty(dir)) return null;
            var candidates = Directory.GetFiles(dir, "*.unityforge.dll");
            if (candidates.Length == 1) return candidates[0];
            return null;
        }

        private T ResolveSymbol<T>(string name) where T : class
        {
            if (!NativeLibrary.TryGetExport(_rustModule, name, out var addr) || addr == IntPtr.Zero)
                return null;
            return Marshal.GetDelegateForFunctionPointer(addr, typeof(T)) as T;
        }
    }

    /// <summary>
    /// IL2CPP-injected MonoBehaviour that drives unityforge_tick
    /// every frame. Held alive by the GameObject created at Load.
    /// </summary>
    public class UnityforgeTickDriver : MonoBehaviour
    {
        public UnityforgeTickDriver(IntPtr ptr) : base(ptr) { }

        // Bare fn-pointer delegate; assigned by the plugin.
        public Action<float> Tick;

        private void Update()
        {
            if (Tick == null) return;
            InputBridge.PollAll();
            try { Tick(Time.realtimeSinceStartup); }
            catch (Exception e) { ShimLogger.Error("Unityforge.Shim.Il2Cpp: tick threw: " + e); }
        }
    }

    internal static class NativeLibrary
    {
        [DllImport("kernel32", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern IntPtr LoadLibraryW(string path);
        [DllImport("kernel32", SetLastError = true, CharSet = CharSet.Ansi)]
        private static extern IntPtr GetProcAddress(IntPtr module, string name);
        [DllImport("kernel32", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool FreeLibrary(IntPtr module);

        public static IntPtr Load(string path) => LoadLibraryW(path);
        public static void Free(IntPtr module) { if (module != IntPtr.Zero) FreeLibrary(module); }
        public static bool TryGetExport(IntPtr module, string name, out IntPtr addr)
        {
            addr = GetProcAddress(module, name);
            return addr != IntPtr.Zero;
        }
    }
}
