// HarmonyBridge.cs. Exposes Harmony patch operations to Rust as
// function pointers.
//
// Rust passes an unmanaged `extern "C" fn` pointer for the
// prefix/postfix body. We wrap it in a managed delegate via
// Marshal.GetDelegateForFunctionPointer and dispatch to it from
// ONE static prefix dispatcher + ONE static postfix dispatcher,
// keyed by the patched method.
//
// Why the dispatcher: Harmony patch methods must be STATIC. The
// first version targeted `new Action(() => del(...)).Method`,
// which is an instance method on a compiler-generated closure
// class; HarmonyLib rejects it, so every Rust-side patch was
// silently failing (todo.md "Next up" item 0). The static
// dispatchers are real static methods Harmony accepts; the
// per-method delegate lists route each call to the right Rust
// fn(s).
//
// Prefix delegate signature: int(IntPtr). Non-zero return = skip
// the original method (matches unityforge/src/hook.rs).
// Postfix delegate signature: void(IntPtr).
// The IntPtr is reserved (always null for now); future extensions
// may pass a per-call context.

using System;
using System.Collections.Generic;
using System.Reflection;
using System.Runtime.InteropServices;
using HarmonyLib;

namespace Unityforge.Shim
{
    public static class HarmonyBridge
    {
        private static readonly object _lock = new object();
        private static readonly Dictionary<int, PatchEntry> _patches = new Dictionary<int, PatchEntry>();
        private static readonly Dictionary<MethodBase, MethodPatches> _byMethod = new Dictionary<MethodBase, MethodPatches>();
        private static int _next = 1;
        private static Harmony _harmony;

        private static readonly MethodInfo PrefixDispatcherMi =
            typeof(HarmonyBridge).GetMethod(nameof(PrefixDispatcher), BindingFlags.NonPublic | BindingFlags.Static);
        private static readonly MethodInfo PostfixDispatcherMi =
            typeof(HarmonyBridge).GetMethod(nameof(PostfixDispatcher), BindingFlags.NonPublic | BindingFlags.Static);

        // delegate signatures matching the Rust extern "C" fns
        private delegate int RustPrefixDelegate(IntPtr ctx);
        private delegate void RustPostfixDelegate(IntPtr ctx);

        public delegate int PatchPrefixFn(IntPtr typeNameUtf8, IntPtr methodNameUtf8, IntPtr rustFnPtr);
        public delegate int PatchPostfixFn(IntPtr typeNameUtf8, IntPtr methodNameUtf8, IntPtr rustFnPtr);
        public delegate void UnpatchFn(int handle);

        public static readonly PatchPrefixFn PatchPrefixDelegate = PatchPrefix;
        public static readonly PatchPostfixFn PatchPostfixDelegate = PatchPostfix;
        public static readonly UnpatchFn UnpatchDelegate = Unpatch;

        public static void EnsureHarmony(string instanceId)
        {
            if (_harmony == null) _harmony = new Harmony(instanceId);
        }

        /// <summary>
        /// Drop every active patch. Used during hot reload so
        /// Harmony doesn't dispatch into a freed Rust DLL.
        /// Per-dispatcher unpatch, not UnpatchSelf: UnpatchSelf is
        /// HarmonyX-only (missing in pardeike Harmony 2.0.4, which
        /// the survivalist host builds against).
        /// </summary>
        public static void UnpatchAll()
        {
            lock (_lock)
            {
                if (_harmony != null)
                {
                    foreach (var kv in _byMethod)
                    {
                        try
                        {
                            if (kv.Value.PrefixApplied) _harmony.Unpatch(kv.Key, PrefixDispatcherMi);
                            if (kv.Value.PostfixApplied) _harmony.Unpatch(kv.Key, PostfixDispatcherMi);
                        }
                        catch (Exception e)
                        {
                            ShimLogger.Error("HarmonyBridge.UnpatchAll: " + e);
                        }
                    }
                }
                _byMethod.Clear();
                _patches.Clear();
            }
        }

        private class PatchEntry
        {
            public MethodBase Target;
            public bool IsPrefix;
            // The managed delegate wrapping the Rust fn pointer.
            // Held here so the GC can't collect it while the
            // patch is live; also the identity used to remove it
            // from the per-method list on Unpatch.
            public object KeepAliveDelegate;
        }

        private class MethodPatches
        {
            public readonly List<RustPrefixDelegate> Prefixes = new List<RustPrefixDelegate>();
            public readonly List<RustPostfixDelegate> Postfixes = new List<RustPostfixDelegate>();
            public bool PrefixApplied;
            public bool PostfixApplied;
        }

        // ---- static dispatchers (the methods Harmony targets) ----------

        private static bool PrefixDispatcher(MethodBase __originalMethod)
        {
            RustPrefixDelegate[] snapshot = null;
            lock (_lock)
            {
                if (__originalMethod != null
                    && _byMethod.TryGetValue(__originalMethod, out var mp)
                    && mp.Prefixes.Count > 0)
                {
                    snapshot = mp.Prefixes.ToArray();
                }
            }
            if (snapshot == null) return true;
            bool runOriginal = true;
            for (int i = 0; i < snapshot.Length; i++)
            {
                try
                {
                    // Non-zero from the Rust prefix = skip the
                    // original (unityforge/src/hook.rs contract).
                    if (snapshot[i](IntPtr.Zero) != 0) runOriginal = false;
                }
                catch (Exception e)
                {
                    ShimLogger.Error("HarmonyBridge: prefix callback threw: " + e);
                }
            }
            return runOriginal;
        }

        private static void PostfixDispatcher(MethodBase __originalMethod)
        {
            RustPostfixDelegate[] snapshot = null;
            lock (_lock)
            {
                if (__originalMethod != null
                    && _byMethod.TryGetValue(__originalMethod, out var mp)
                    && mp.Postfixes.Count > 0)
                {
                    snapshot = mp.Postfixes.ToArray();
                }
            }
            if (snapshot == null) return;
            for (int i = 0; i < snapshot.Length; i++)
            {
                try { snapshot[i](IntPtr.Zero); }
                catch (Exception e)
                {
                    ShimLogger.Error("HarmonyBridge: postfix callback threw: " + e);
                }
            }
        }

        // ---- Rust-facing entry points -----------------------------------

        private static int PatchPrefix(IntPtr typeNameUtf8, IntPtr methodNameUtf8, IntPtr rustFnPtr)
        {
            try
            {
                if (_harmony == null || rustFnPtr == IntPtr.Zero) return 0;
                var target = ResolveTarget(typeNameUtf8, methodNameUtf8);
                if (target == null) return 0;
                var del = (RustPrefixDelegate)Marshal.GetDelegateForFunctionPointer(rustFnPtr, typeof(RustPrefixDelegate));

                int handle;
                lock (_lock)
                {
                    var mp = GetOrAddMethodPatches(target);
                    if (!mp.PrefixApplied)
                    {
                        // One dispatcher patch per method; further
                        // prefixes on the same method just join the
                        // delegate list.
                        _harmony.Patch(target, prefix: new HarmonyMethod(PrefixDispatcherMi));
                        mp.PrefixApplied = true;
                    }
                    mp.Prefixes.Add(del);
                    handle = _next++;
                    _patches[handle] = new PatchEntry { Target = target, IsPrefix = true, KeepAliveDelegate = del };
                }
                return handle;
            }
            catch (Exception e)
            {
                ShimLogger.Error("HarmonyBridge.PatchPrefix: " + e);
                return 0;
            }
        }

        private static int PatchPostfix(IntPtr typeNameUtf8, IntPtr methodNameUtf8, IntPtr rustFnPtr)
        {
            try
            {
                if (_harmony == null || rustFnPtr == IntPtr.Zero) return 0;
                var target = ResolveTarget(typeNameUtf8, methodNameUtf8);
                if (target == null) return 0;
                var del = (RustPostfixDelegate)Marshal.GetDelegateForFunctionPointer(rustFnPtr, typeof(RustPostfixDelegate));

                int handle;
                lock (_lock)
                {
                    var mp = GetOrAddMethodPatches(target);
                    if (!mp.PostfixApplied)
                    {
                        _harmony.Patch(target, postfix: new HarmonyMethod(PostfixDispatcherMi));
                        mp.PostfixApplied = true;
                    }
                    mp.Postfixes.Add(del);
                    handle = _next++;
                    _patches[handle] = new PatchEntry { Target = target, IsPrefix = false, KeepAliveDelegate = del };
                }
                return handle;
            }
            catch (Exception e)
            {
                ShimLogger.Error("HarmonyBridge.PatchPostfix: " + e);
                return 0;
            }
        }

        private static void Unpatch(int handle)
        {
            lock (_lock)
            {
                if (!_patches.TryGetValue(handle, out var entry)) return;
                _patches.Remove(handle);
                if (!_byMethod.TryGetValue(entry.Target, out var mp)) return;
                if (entry.IsPrefix)
                {
                    mp.Prefixes.Remove((RustPrefixDelegate)entry.KeepAliveDelegate);
                    if (mp.Prefixes.Count == 0 && mp.PrefixApplied)
                    {
                        try { _harmony?.Unpatch(entry.Target, PrefixDispatcherMi); }
                        catch (Exception e) { ShimLogger.Error("HarmonyBridge.Unpatch: " + e); }
                        mp.PrefixApplied = false;
                    }
                }
                else
                {
                    mp.Postfixes.Remove((RustPostfixDelegate)entry.KeepAliveDelegate);
                    if (mp.Postfixes.Count == 0 && mp.PostfixApplied)
                    {
                        try { _harmony?.Unpatch(entry.Target, PostfixDispatcherMi); }
                        catch (Exception e) { ShimLogger.Error("HarmonyBridge.Unpatch: " + e); }
                        mp.PostfixApplied = false;
                    }
                }
                if (!mp.PrefixApplied && !mp.PostfixApplied)
                {
                    _byMethod.Remove(entry.Target);
                }
            }
        }

        private static MethodBase ResolveTarget(IntPtr typeNameUtf8, IntPtr methodNameUtf8)
        {
            var tname = Marshal.PtrToStringAnsi(typeNameUtf8);
            var mname = Marshal.PtrToStringAnsi(methodNameUtf8);
            var t = TypeCache.Resolve(tname);
            if (t == null) return null;
            return AccessTools.Method(t, mname);
        }

        private static MethodPatches GetOrAddMethodPatches(MethodBase target)
        {
            if (!_byMethod.TryGetValue(target, out var mp))
            {
                mp = new MethodPatches();
                _byMethod[target] = mp;
            }
            return mp;
        }
    }
}
