// Plugin.cs. BepInEx entry. Wires the log sink to BepInEx's
// ManualLogSource, locates the Rust cdylib next to this DLL, and
// drives the shared GenerationLoader (cs-shim-common) from a
// MonoBehaviour's Update. Generation loading + hot reload live in
// GenerationLoader; this file owns only the BepInEx host seam and
// the WWM-specific patches.
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

            HarmonyBridge.EnsureHarmony(PluginGuid);

            // WWM-specific: block the demo-end panel by patching
            // DemoCompleteScreenUI.Show with a static prefix that
            // returns false. The Rust-side patch path uses an
            // instance-method bridge which HarmonyX rejects; until
            // that lands, this direct shim-side patch unblocks
            // gameplay so the demo screen never opens.
            InstallDemoCompleteBlock();

            _loader = new GenerationLoader(new MonoBackendBridge(), MonoBridge.ClearHandles);
            if (!_loader.LoadInitial(dllPath))
            {
                ShimLogger.Error("Unityforge.Shim: initial generation failed to load");
                _loader = null;
                return;
            }
            ShimLogger.Info("Unityforge.Shim: ready (generation 0)");
        }

        private static void InstallDemoCompleteBlock()
        {
            // Patch upstream per Harmony edge-cases doc. The
            // panel itself is opened by an asset-level event;
            // patching the panel's lifecycle methods is forbidden
            // (Harmony issue #374. Unity caches MonoBehaviour
            // method pointers and loses them after a patch). The
            // upstream caller is the SellGoldBar tutorial task's
            // condition-check / on-finish method. Patching it
            // with `__instance.name == "SellGoldBar"` filter
            // prevents the task from ever firing onFinishEvent.
            try
            {
                if (_wwmHarmony == null)
                    _wwmHarmony = new HarmonyLib.Harmony("abix.unityforge.shim.wwmblock");

                // Direct upstream patch: TutorialManager.CompleteDemo
                // (and CompleteDemoCoroutine) are the methods that
                // fire the demo-complete screen, including on save
                // reload when tutorialCurrentStep is already past
                // the threshold. Confirmed via list_methods.
                PatchSingle("TutorialManager", "CompleteDemo");
                PatchSingle("TutorialManager", "CompleteDemoCoroutine");

                // Defense in depth: also patch the sell task in
                // case the user is on a fresh save that hasn't
                // turned in the gold bars yet.
                int patched = PatchTaskClass("TutorialTaskSellItem");
                int patchedBase = PatchTaskClass("TutorialTask");
                ShimLogger.Info($"WWM block: patched {patched} on TutorialTaskSellItem, {patchedBase} on TutorialTask");
            }
            catch (Exception e)
            {
                ShimLogger.Error("WWM block: install threw: " + e);
            }
        }

        private static readonly HashSet<string> _wwmSkipMethodNames = new HashSet<string>
        {
            // Unity lifecycle. Patching these breaks Unity's
            // method-pointer cache (Harmony #374).
            "Awake", "Start", "Update", "FixedUpdate", "LateUpdate",
            "OnEnable", "OnDisable", "OnDestroy", "OnGUI",
            "OnTriggerEnter", "OnTriggerExit", "OnCollisionEnter",
            // Trivial / safe to leave alone.
            "ToString", "GetHashCode", "Equals",
        };

        private static void PatchSingle(string typeName, string methodName)
        {
            try
            {
                var t = TypeCache.Resolve(typeName);
                if (t == null)
                {
                    ShimLogger.Warn($"WWM block: type {typeName} not found");
                    return;
                }
                var m = HarmonyLib.AccessTools.Method(t, methodName);
                if (m == null)
                {
                    ShimLogger.Warn($"WWM block: {typeName}.{methodName} not found");
                    return;
                }
                _wwmHarmony.Patch(m, prefix: new HarmonyLib.HarmonyMethod(
                    typeof(UnityforgeShimPlugin),
                    nameof(WwmCompleteDemo_Prefix)));
                ShimLogger.Info($"WWM block: patched {typeName}.{methodName} (return false)");
            }
            catch (Exception e)
            {
                ShimLogger.Error($"WWM block: patch {typeName}.{methodName} threw: " + e);
            }
        }

        public static bool WwmCompleteDemo_Prefix(System.Reflection.MethodBase __originalMethod)
        {
            ShimLogger.Info($"WWM block: intercepted {__originalMethod?.DeclaringType?.Name}.{__originalMethod?.Name}() -- demo complete blocked");
            return false;
        }

        private static int PatchTaskClass(string typeName)
        {
            var t = TypeCache.Resolve(typeName);
            if (t == null)
            {
                ShimLogger.Warn($"WWM block: type {typeName} not found");
                return 0;
            }
            var prefix = new HarmonyLib.HarmonyMethod(
                typeof(UnityforgeShimPlugin),
                nameof(WwmTaskMethod_Prefix));
            int count = 0;
            var methods = t.GetMethods(System.Reflection.BindingFlags.Public
                | System.Reflection.BindingFlags.NonPublic
                | System.Reflection.BindingFlags.Instance
                | System.Reflection.BindingFlags.DeclaredOnly);
            foreach (var m in methods)
            {
                if (m.IsAbstract) continue;
                if (m.Name.StartsWith("get_") || m.Name.StartsWith("set_")) continue;
                if (m.Name.StartsWith("add_") || m.Name.StartsWith("remove_")) continue;
                if (_wwmSkipMethodNames.Contains(m.Name)) continue;
                try
                {
                    _wwmHarmony.Patch(m, prefix: prefix);
                    count++;
                }
                catch (Exception e)
                {
                    ShimLogger.Warn($"WWM block: patch {typeName}.{m.Name} threw: {e.Message}");
                }
            }
            return count;
        }

        private static int _wwmInterceptCount;
        public static bool WwmTaskMethod_Prefix(
            UnityEngine.MonoBehaviour __instance,
            System.Reflection.MethodBase __originalMethod)
        {
            try
            {
                if (__instance == null || __instance.name != "SellGoldBar") return true;
                _wwmInterceptCount++;
                if (_wwmInterceptCount <= 5)
                {
                    ShimLogger.Info(
                        $"WWM block: intercepted SellGoldBar.{__originalMethod?.Name}() #{_wwmInterceptCount}");
                }
                return false; // skip original on the SellGoldBar task
            }
            catch (Exception e)
            {
                ShimLogger.Error("WWM block: prefix threw: " + e);
                return true;
            }
        }

        private static bool _demoBlockInstalled;
        private static void OnSceneLoadedTryBlock(
            UnityEngine.SceneManagement.Scene scene,
            UnityEngine.SceneManagement.LoadSceneMode mode)
        {
            if (_demoBlockInstalled) return;
            try
            {
                var t = TypeCache.Resolve("DemoCompleteScreenUI");
                if (t == null) return;
                DoPatch(t);
                _demoBlockInstalled = true;
                UnityEngine.SceneManagement.SceneManager.sceneLoaded -= OnSceneLoadedTryBlock;
            }
            catch (Exception e)
            {
                ShimLogger.Error("WWM block: scene-load retry threw: " + e);
            }
        }

        private static HarmonyLib.Harmony _wwmHarmony;
        private static readonly string[] _wwmTargetMethods = new[]
        {
            "Update", "Show", "FocusCoroutine",
            "GetEscapeButtonName", "UpdateEscape",
        };

        private static void DoPatch(Type t)
        {
            if (_wwmHarmony == null)
                _wwmHarmony = new HarmonyLib.Harmony("abix.unityforge.shim.wwmblock");
            int patched = 0;
            foreach (var name in _wwmTargetMethods)
            {
                try
                {
                    var m = HarmonyLib.AccessTools.Method(t, name);
                    if (m == null)
                    {
                        ShimLogger.Warn($"WWM block: method {name} not found");
                        continue;
                    }
                    _wwmHarmony.Patch(m, prefix: new HarmonyLib.HarmonyMethod(
                        typeof(UnityforgeShimPlugin), nameof(BlockDemoComplete_UpdatePrefix)));
                    ShimLogger.Info($"WWM block: patched {name}");
                    patched++;
                }
                catch (Exception e)
                {
                    ShimLogger.Error($"WWM block: patch {name} threw: " + e);
                }
            }
            _demoBlockInstalled = patched > 0;
            ShimLogger.Info($"WWM block: total patched = {patched}");
        }

        // Universal prefix used for every patched method on
        // DemoCompleteScreenUI. Whichever method fires first
        // deactivates the panel GameObject and stops the original
        // from running. The first log line tells us which method
        // Unity actually invokes; that diagnostic is the point.
        public static bool BlockDemoComplete_UpdatePrefix(
            UnityEngine.MonoBehaviour __instance,
            System.Reflection.MethodBase __originalMethod)
        {
            try
            {
                string mname = __originalMethod != null ? __originalMethod.Name : "<unknown>";
                ShimLogger.Info($"WWM block: intercepted {mname}() on DemoCompleteScreenUI");
                if (__instance != null && __instance.gameObject != null
                    && __instance.gameObject.activeSelf)
                {
                    __instance.gameObject.SetActive(false);
                    ShimLogger.Info("WWM block: deactivated DemoCompleteScreen GameObject");
                }
            }
            catch (Exception e)
            {
                ShimLogger.Error("WWM block: prefix threw: " + e);
            }
            return false; // skip original
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
