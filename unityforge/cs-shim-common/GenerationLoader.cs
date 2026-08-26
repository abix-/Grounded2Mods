// GenerationLoader.cs. Host-agnostic generation-versioned loader
// for the Rust cdylib. Extracted from cs-shim-mono/Plugin.cs so
// every host (BepInEx plugin, a game's official mod loader)
// shares one implementation.
//
// A host:
//   1. sets ShimLogger.Sink,
//   2. constructs a GenerationLoader with its backend bridge and
//      a backend-handle-clear callback,
//   3. calls LoadInitial(dllPath) once,
//   4. calls Tick(now) every frame (after InputBridge.PollAll()),
//   5. calls ShutdownFinal() on process teardown, or
//      ShutdownForUnload() + ReinitAfterUnload() around a host
//      unload/reload cycle where the process keeps running.
//
// Hot reload: generation-versioned. Each iteration drops a
// `*.gen<N>.dll` next to the canonical DLL. The per-second
// watcher picks it up, calls `unityforge_shutdown` on the active
// generation (which runs the modforge shutdown registry. HTTP
// server unblock + slot poller wake + thread joins. So all
// background threads exit before we proceed), then `LoadLibrary`s
// the new generation, calls its `unityforge_init`, and switches
// active. The OLD module is never FreeLibrary'd; the OS unmaps it
// on its own schedule once nothing references it.
//
// See docs/unityforge-plan.md section 6.5 "Hot reload" for the
// full design + rationale.

using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text.RegularExpressions;

namespace Unityforge.Shim
{
    public sealed class GenerationLoader
    {
        public const string TargetEnv = "UNITYFORGE_TARGET";

        // P/Invoke targets resolved by GetProcAddress at runtime.
        private delegate int UnityforgeInitFn(IntPtr bridge);
        private delegate void UnityforgeTickFn(float now);
        private delegate void UnityforgeShutdownFn();

        /// <summary>
        /// One loaded image of the Rust cdylib. The loader holds
        /// at most one active generation at a time; old
        /// generations are dropped into `_quiesced` once their
        /// background threads have been signaled to stop and
        /// joined (by `Shutdown()`). We never FreeLibrary. the OS
        /// unmaps the image once nothing references its code
        /// segment.
        /// </summary>
        private class Generation
        {
            public int N;                                 // 0 = initial, then 1, 2, ...
            public string Path;
            public IntPtr Module;
            public UnityforgeInitFn Init;
            public UnityforgeTickFn Tick;
            public UnityforgeShutdownFn Shutdown;
            public BridgeTable Bridge;
            public GCHandle BridgeHandle;                 // pinned pointer passed to Rust
        }

        private readonly IBackendBridge _backend;
        private readonly Action _clearBackendHandles;
        private Generation _active;
        private Generation _dormant;                      // parked by ShutdownForUnload for re-init
        private readonly List<Generation> _quiesced = new List<Generation>();
        private string _canonicalDir;
        private float _lastReloadCheck;
        private const float ReloadCheckIntervalSec = 1.0f;
        private static readonly Regex GenFilenameRe = new Regex(
            @"\.gen(\d+)\.dll$", RegexOptions.IgnoreCase);

        public event Action<IntPtr> GenerationActivated;

        public GenerationLoader(IBackendBridge backend, Action clearBackendHandles)
        {
            _backend = backend;
            _clearBackendHandles = clearBackendHandles;
        }

        public bool Active => _active != null;

        /// <summary>
        /// Find the canonical `*.unityforge.dll` in `dir`
        /// (generation files excluded; they are picked up by the
        /// hot-reload watcher, not at initial load). The
        /// UNITYFORGE_TARGET env var overrides. Returns null
        /// unless exactly one candidate exists.
        /// </summary>
        public static string LocateRustDll(string dir)
        {
            var explicitTarget = Environment.GetEnvironmentVariable(TargetEnv);
            if (!string.IsNullOrEmpty(explicitTarget) && File.Exists(explicitTarget))
                return explicitTarget;
            if (string.IsNullOrEmpty(dir)) return null;
            var candidates = Directory.GetFiles(dir, "*.unityforge.dll")
                .Where(f => !GenFilenameRe.IsMatch(f))
                .ToArray();
            if (candidates.Length == 1) return candidates[0];
            return null;
        }

        public bool LoadInitial(string dllPath)
        {
            _canonicalDir = Path.GetDirectoryName(dllPath);
            ShimLogger.Info("Unityforge.Shim: loading " + dllPath);
            _active = LoadGeneration(dllPath, generationNumber: 0);
            NotifyGenerationActivated(_active);
            return _active != null;
        }

        /// <summary>
        /// Per-frame drive: hot-reload check + tick the active
        /// generation. The host calls InputBridge.PollAll() itself
        /// before this (input is not the loader's concern).
        /// </summary>
        public void Tick(float now)
        {
            if (_active == null) return;
            CheckHotReload(now);
            if (_active == null) return; // reload may have left us unactive
            try { _active.Tick(now); }
            catch (Exception e) { ShimLogger.Error("Unityforge.Shim: tick threw: " + e); }
        }

        /// <summary>
        /// Process-teardown shutdown (BepInEx OnDestroy). Frees
        /// pinned bridge handles; the generation cannot be
        /// re-initialized afterwards.
        /// </summary>
        public void ShutdownFinal()
        {
            if (_active != null)
            {
                try { _active.Shutdown(); }
                catch (Exception e) { ShimLogger.Error("Unityforge.Shim: shutdown threw: " + e); }
                if (_active.BridgeHandle.IsAllocated) _active.BridgeHandle.Free();
                _active = null;
            }
            if (_dormant != null)
            {
                if (_dormant.BridgeHandle.IsAllocated) _dormant.BridgeHandle.Free();
                _dormant = null;
            }
            foreach (var g in _quiesced)
            {
                if (g.BridgeHandle.IsAllocated) g.BridgeHandle.Free();
            }
            _quiesced.Clear();
            // Intentionally NO FreeLibrary calls. Process exit
            // unmaps everything; before that, old generations'
            // threads may still be exiting on stop signals.
        }

        /// <summary>
        /// Host-unload shutdown (a game's official mod loader
        /// calling Unload while the process keeps running). Runs
        /// the Rust shutdown registry and clears bindings/handles,
        /// but KEEPS the module mapped and the bridge pinned so a
        /// later ReinitAfterUnload() can re-arm the same image.
        /// </summary>
        public void ShutdownForUnload()
        {
            var g = _active;
            if (g == null) return;
            _active = null;
            try { g.Shutdown(); }
            catch (Exception e) { ShimLogger.Error("Unityforge.Shim: unload shutdown threw: " + e); }
            InputBridge.Clear();
            _clearBackendHandles?.Invoke();
            _dormant = g;
            ShimLogger.Info($"Unityforge.Shim: generation {g.N} parked for reload");
        }

        /// <summary>
        /// Re-arm the generation parked by ShutdownForUnload().
        /// Its threads are gone but the code is still mapped; we
        /// can call init again (same move as the hot-reload
        /// rollback path).
        /// </summary>
        public bool ReinitAfterUnload()
        {
            if (_active != null) return true;
            var g = _dormant;
            if (g == null) return false;
            try
            {
                int rc = g.Init(g.BridgeHandle.AddrOfPinnedObject());
                if (rc != 0)
                {
                    ShimLogger.Error($"Unityforge.Shim: re-init of generation {g.N} returned " + rc);
                    return false;
                }
            }
            catch (Exception e)
            {
                ShimLogger.Error($"Unityforge.Shim: re-init of generation {g.N} threw: " + e);
                return false;
            }
            _dormant = null;
            _active = g;
            NotifyGenerationActivated(g);
            ShimLogger.Info($"Unityforge.Shim: generation {g.N} re-armed");
            return true;
        }

        // ---- hot reload --------------------------------------------------

        private void CheckHotReload(float now)
        {
            if (now - _lastReloadCheck < ReloadCheckIntervalSec) return;
            _lastReloadCheck = now;
            if (string.IsNullOrEmpty(_canonicalDir)) return;

            // Find the highest .gen<N>.dll in the canonical dir.
            // We swap to whichever generation is newest on disk
            // higher than the active one. Lower-numbered files
            // are ignored (stale staging from a prior run).
            string[] candidates;
            try { candidates = Directory.GetFiles(_canonicalDir, "*.gen*.dll"); }
            catch { return; }

            int bestN = _active.N;
            string bestPath = null;
            foreach (var c in candidates)
            {
                var m = GenFilenameRe.Match(c);
                if (!m.Success) continue;
                if (!int.TryParse(m.Groups[1].Value, out var n)) continue;
                if (n > bestN) { bestN = n; bestPath = c; }
            }
            if (bestPath == null) return;

            ShimLogger.Info(
                $"Unityforge.Shim: hot reload generation {_active.N} -> {bestN}");
            HotSwap(bestN, bestPath);
        }

        private void HotSwap(int newGen, string newPath)
        {
            var old = _active;

            // Step 1: stop ticking the old generation. We do
            // this BEFORE touching the new image so a long shim
            // shutdown doesn't double-fire ops while new is
            // half-init.
            _active = null;

            // Step 2: graceful shutdown on the old generation.
            // This runs the Rust SHUTDOWN_REGISTRY which:
            //  - server::shutdown_all unblocks tiny_http and
            //    joins the listener thread
            //  - rpg::poller::shutdown_all wakes the poller's
            //    condvar and joins
            //  - HOOK_REGISTRY.shutdown_all unpatches Harmony
            // After this call returns, no Rust thread from the
            // old generation should be executing in its code
            // segment.
            try { old.Shutdown(); }
            catch (Exception e)
            {
                ShimLogger.Error(
                    $"Unityforge.Shim: old gen {old.N} shutdown threw: " + e);
                // Continue anyway. Threads MAY still be
                // exiting; we just don't FreeLibrary so the
                // worst case is they finish executing into a
                // still-mapped image.
            }

            // Step 3: clear input bindings + backend handle table.
            // Harmony patches were already unpatched per-handle
            // by Rust's HOOK_REGISTRY.shutdown_all (which calls
            // back into HarmonyBridge.Unpatch for each one).
            // Calling _harmony.UnpatchSelf() here in addition
            // tries to detour-back already-cleaned methods and
            // hits "IL Compile Error" inside HarmonyX. Skip it;
            // the per-handle path is sufficient and the
            // _patches dictionary is already empty.
            InputBridge.Clear();
            _clearBackendHandles?.Invoke();

            // Step 4: park the old generation in `_quiesced`.
            // We keep its module mapped (no FreeLibrary) so any
            // stray thread that didn't quite finish exiting can
            // still run its last instructions safely.
            _quiesced.Add(old);

            // Step 5: load the new image.
            var fresh = LoadGeneration(newPath, newGen);
            if (fresh == null)
            {
                ShimLogger.Error(
                    $"Unityforge.Shim: gen {newGen} failed to load; rolling back");
                // Re-arm the old generation. Its threads are
                // gone but the code is still mapped; we can
                // call init again.
                _quiesced.Remove(old);
                try
                {
                    int rc = old.Init(old.BridgeHandle.AddrOfPinnedObject());
                    if (rc == 0)
                    {
                        _active = old;
                        NotifyGenerationActivated(old);
                        return;
                    }
                }
                catch (Exception e)
                {
                    ShimLogger.Error(
                        "Unityforge.Shim: rollback re-init threw: " + e);
                }
                return;
            }

            _active = fresh;
            NotifyGenerationActivated(fresh);
            ShimLogger.Info(
                $"Unityforge.Shim: hot reload complete (now generation {newGen}; {_quiesced.Count} draining)");
        }

        private void NotifyGenerationActivated(Generation generation)
        {
            if (generation == null || GenerationActivated == null) return;
            try { GenerationActivated(generation.Module); }
            catch (Exception e)
            {
                ShimLogger.Error("Unityforge.Shim: generation binding failed: " + e);
            }
        }

        private Generation LoadGeneration(string path, int generationNumber)
        {
            var module = NativeLibrary.Load(path);
            if (module == IntPtr.Zero)
            {
                ShimLogger.Error(
                    $"Unityforge.Shim: LoadLibrary failed for {path}: " + Marshal.GetLastWin32Error());
                return null;
            }
            var gen = new Generation { N = generationNumber, Path = path, Module = module };
            gen.Init = ResolveSymbol<UnityforgeInitFn>(module, "unityforge_init");
            gen.Tick = ResolveSymbol<UnityforgeTickFn>(module, "unityforge_tick");
            gen.Shutdown = ResolveSymbol<UnityforgeShutdownFn>(module, "unityforge_shutdown");
            if (gen.Init == null || gen.Tick == null || gen.Shutdown == null)
            {
                ShimLogger.Error(
                    $"Unityforge.Shim: gen {generationNumber} DLL is missing one of unityforge_init / unityforge_tick / unityforge_shutdown");
                return null;
            }

            // Each generation gets its own pinned BridgeTable
            // instance. The function pointers inside point at
            // the same shared C# delegates; the struct itself
            // lives separately so the Rust side's pointer
            // stays valid for the lifetime of that generation.
            gen.Bridge = Bridge.Build(_backend);
            gen.BridgeHandle = GCHandle.Alloc(gen.Bridge, GCHandleType.Pinned);

            try
            {
                int rc = gen.Init(gen.BridgeHandle.AddrOfPinnedObject());
                if (rc != 0)
                {
                    ShimLogger.Error(
                        $"Unityforge.Shim: gen {generationNumber} unityforge_init returned " + rc);
                    if (gen.BridgeHandle.IsAllocated) gen.BridgeHandle.Free();
                    return null;
                }
            }
            catch (Exception e)
            {
                ShimLogger.Error(
                    $"Unityforge.Shim: gen {generationNumber} unityforge_init threw: " + e);
                if (gen.BridgeHandle.IsAllocated) gen.BridgeHandle.Free();
                return null;
            }
            return gen;
        }

        private T ResolveSymbol<T>(IntPtr module, string name) where T : class
        {
            if (!NativeLibrary.TryGetExport(module, name, out var addr) || addr == IntPtr.Zero)
                return null;
            return Marshal.GetDelegateForFunctionPointer(addr, typeof(T)) as T;
        }
    }

    internal static class NativeLibrary
    {
        [DllImport("kernel32", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern IntPtr LoadLibraryW(string path);
        [DllImport("kernel32", SetLastError = true, CharSet = CharSet.Ansi)]
        private static extern IntPtr GetProcAddress(IntPtr module, string name);

        public static IntPtr Load(string path) => LoadLibraryW(path);
        public static bool TryGetExport(IntPtr module, string name, out IntPtr addr)
        {
            addr = GetProcAddress(module, name);
            return addr != IntPtr.Zero;
        }
        // No Free(): generation-versioned loading never
        // FreeLibrary's an old image.
    }
}
