// HarmonyBridge.cs. Exposes Harmony patch operations to Rust as
// function pointers.
//
// Rust passes an unmanaged `extern "C" fn` pointer for the
// prefix/postfix body. We wrap it in a managed delegate via
// Marshal.GetDelegateForFunctionPointer and dispatch to it from
// static dispatcher methods keyed by the patched method.
//
// Why dispatchers: Harmony patch methods must be STATIC. The
// first version targeted `new Action(() => del(...)).Method`,
// which is an instance method on a compiler-generated closure
// class; HarmonyLib rejects it, so every Rust-side patch was
// silently failing (todo.md "Next up" item 0). The static
// dispatchers are real static methods Harmony accepts; the
// per-method delegate lists route each call to the right Rust
// fn(s).
//
// Patch kinds:
//   - prefix:  int(IntPtr ctx). Non-zero return = skip the
//     original method (matches unityforge/src/hook.rs). ctx is
//     IntPtr.Zero for the plain kind.
//   - postfix: void(IntPtr ctx). ctx is IntPtr.Zero.
//   - prefix_ctx (v5): int(IntPtr ctx) where ctx carries a FRESH
//     object handle for the patch's context object: ctxKind 0 =
//     __instance (instance methods only), ctxKind 1 = args[0].
//     The Rust callback OWNS the handle and must release it
//     (MonoObject::from_handle + Drop does). Zero when the
//     context object is null.

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
        private static readonly MethodInfo PrefixInstanceCtxDispatcherMi =
            typeof(HarmonyBridge).GetMethod(nameof(PrefixInstanceCtxDispatcher), BindingFlags.NonPublic | BindingFlags.Static);
        private static readonly MethodInfo PrefixArg0CtxDispatcherMi =
            typeof(HarmonyBridge).GetMethod(nameof(PrefixArg0CtxDispatcher), BindingFlags.NonPublic | BindingFlags.Static);

        // delegate signatures matching the Rust extern "C" fns
        private delegate int RustPrefixDelegate(IntPtr ctx);
        private delegate void RustPostfixDelegate(IntPtr ctx);

        public delegate int PatchPrefixFn(IntPtr typeNameUtf8, IntPtr methodNameUtf8, IntPtr rustFnPtr);
        public delegate int PatchPostfixFn(IntPtr typeNameUtf8, IntPtr methodNameUtf8, IntPtr rustFnPtr);
        public delegate int PatchPrefixCtxFn(IntPtr typeNameUtf8, IntPtr methodNameUtf8, int ctxKind, IntPtr rustFnPtr);
        public delegate void UnpatchFn(int handle);

        public static readonly PatchPrefixFn PatchPrefixDelegate = PatchPrefix;
        public static readonly PatchPostfixFn PatchPostfixDelegate = PatchPostfix;
        public static readonly PatchPrefixCtxFn PatchPrefixCtxDelegate = PatchPrefixCtx;
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
                            if (kv.Value.PrefixInstanceCtxApplied) _harmony.Unpatch(kv.Key, PrefixInstanceCtxDispatcherMi);
                            if (kv.Value.PrefixArg0CtxApplied) _harmony.Unpatch(kv.Key, PrefixArg0CtxDispatcherMi);
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

        private enum PatchKind
        {
            Prefix,
            Postfix,
            PrefixInstanceCtx,
            PrefixArg0Ctx,
        }

        private class PatchEntry
        {
            public MethodBase Target;
            public PatchKind Kind;
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
            public readonly List<RustPrefixDelegate> PrefixesInstanceCtx = new List<RustPrefixDelegate>();
            public readonly List<RustPrefixDelegate> PrefixesArg0Ctx = new List<RustPrefixDelegate>();
            public bool PrefixApplied;
            public bool PostfixApplied;
            public bool PrefixInstanceCtxApplied;
            public bool PrefixArg0CtxApplied;

            public bool AnyApplied => PrefixApplied || PostfixApplied
                || PrefixInstanceCtxApplied || PrefixArg0CtxApplied;
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

        private static bool PrefixInstanceCtxDispatcher(object __instance, MethodBase __originalMethod)
        {
            RustPrefixDelegate[] snapshot = null;
            lock (_lock)
            {
                if (__originalMethod != null
                    && _byMethod.TryGetValue(__originalMethod, out var mp)
                    && mp.PrefixesInstanceCtx.Count > 0)
                {
                    snapshot = mp.PrefixesInstanceCtx.ToArray();
                }
            }
            if (snapshot == null) return true;
            return DispatchPrefixCtx(snapshot, __instance);
        }

        private static bool PrefixArg0CtxDispatcher(object[] __args, MethodBase __originalMethod)
        {
            RustPrefixDelegate[] snapshot = null;
            lock (_lock)
            {
                if (__originalMethod != null
                    && _byMethod.TryGetValue(__originalMethod, out var mp)
                    && mp.PrefixesArg0Ctx.Count > 0)
                {
                    snapshot = mp.PrefixesArg0Ctx.ToArray();
                }
            }
            if (snapshot == null) return true;
            object ctx = (__args != null && __args.Length > 0) ? __args[0] : null;
            return DispatchPrefixCtx(snapshot, ctx);
        }

        private static bool DispatchPrefixCtx(RustPrefixDelegate[] snapshot, object ctx)
        {
            bool runOriginal = true;
            for (int i = 0; i < snapshot.Length; i++)
            {
                // A FRESH handle per callback: the Rust side owns
                // it and releases it (MonoObject Drop).
                var handle = (ctx != null) ? MonoBridge.Acquire(ctx) : 0;
                try
                {
                    if (snapshot[i](new IntPtr(handle)) != 0) runOriginal = false;
                }
                catch (Exception e)
                {
                    ShimLogger.Error("HarmonyBridge: prefix_ctx callback threw: " + e);
                }
            }
            return runOriginal;
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
                    _patches[handle] = new PatchEntry { Target = target, Kind = PatchKind.Prefix, KeepAliveDelegate = del };
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
                    _patches[handle] = new PatchEntry { Target = target, Kind = PatchKind.Postfix, KeepAliveDelegate = del };
                }
                return handle;
            }
            catch (Exception e)
            {
                ShimLogger.Error("HarmonyBridge.PatchPostfix: " + e);
                return 0;
            }
        }

        private static int PatchPrefixCtx(IntPtr typeNameUtf8, IntPtr methodNameUtf8, int ctxKind, IntPtr rustFnPtr)
        {
            try
            {
                if (_harmony == null || rustFnPtr == IntPtr.Zero) return 0;
                if (ctxKind != 0 && ctxKind != 1) return 0;
                var target = ResolveTarget(typeNameUtf8, methodNameUtf8);
                if (target == null) return 0;
                if (ctxKind == 0 && target.IsStatic) return 0; // __instance needs an instance method
                var del = (RustPrefixDelegate)Marshal.GetDelegateForFunctionPointer(rustFnPtr, typeof(RustPrefixDelegate));

                int handle;
                lock (_lock)
                {
                    var mp = GetOrAddMethodPatches(target);
                    if (ctxKind == 0)
                    {
                        if (!mp.PrefixInstanceCtxApplied)
                        {
                            _harmony.Patch(target, prefix: new HarmonyMethod(PrefixInstanceCtxDispatcherMi));
                            mp.PrefixInstanceCtxApplied = true;
                        }
                        mp.PrefixesInstanceCtx.Add(del);
                    }
                    else
                    {
                        if (!mp.PrefixArg0CtxApplied)
                        {
                            _harmony.Patch(target, prefix: new HarmonyMethod(PrefixArg0CtxDispatcherMi));
                            mp.PrefixArg0CtxApplied = true;
                        }
                        mp.PrefixesArg0Ctx.Add(del);
                    }
                    handle = _next++;
                    _patches[handle] = new PatchEntry
                    {
                        Target = target,
                        Kind = (ctxKind == 0) ? PatchKind.PrefixInstanceCtx : PatchKind.PrefixArg0Ctx,
                        KeepAliveDelegate = del,
                    };
                }
                return handle;
            }
            catch (Exception e)
            {
                ShimLogger.Error("HarmonyBridge.PatchPrefixCtx: " + e);
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
                switch (entry.Kind)
                {
                    case PatchKind.Prefix:
                        mp.Prefixes.Remove((RustPrefixDelegate)entry.KeepAliveDelegate);
                        if (mp.Prefixes.Count == 0 && mp.PrefixApplied)
                        {
                            TryUnpatch(entry.Target, PrefixDispatcherMi);
                            mp.PrefixApplied = false;
                        }
                        break;
                    case PatchKind.Postfix:
                        mp.Postfixes.Remove((RustPostfixDelegate)entry.KeepAliveDelegate);
                        if (mp.Postfixes.Count == 0 && mp.PostfixApplied)
                        {
                            TryUnpatch(entry.Target, PostfixDispatcherMi);
                            mp.PostfixApplied = false;
                        }
                        break;
                    case PatchKind.PrefixInstanceCtx:
                        mp.PrefixesInstanceCtx.Remove((RustPrefixDelegate)entry.KeepAliveDelegate);
                        if (mp.PrefixesInstanceCtx.Count == 0 && mp.PrefixInstanceCtxApplied)
                        {
                            TryUnpatch(entry.Target, PrefixInstanceCtxDispatcherMi);
                            mp.PrefixInstanceCtxApplied = false;
                        }
                        break;
                    case PatchKind.PrefixArg0Ctx:
                        mp.PrefixesArg0Ctx.Remove((RustPrefixDelegate)entry.KeepAliveDelegate);
                        if (mp.PrefixesArg0Ctx.Count == 0 && mp.PrefixArg0CtxApplied)
                        {
                            TryUnpatch(entry.Target, PrefixArg0CtxDispatcherMi);
                            mp.PrefixArg0CtxApplied = false;
                        }
                        break;
                }
                if (!mp.AnyApplied)
                {
                    _byMethod.Remove(entry.Target);
                }
            }
        }

        private static void TryUnpatch(MethodBase target, MethodInfo dispatcher)
        {
            try { _harmony?.Unpatch(target, dispatcher); }
            catch (Exception e) { ShimLogger.Error("HarmonyBridge.Unpatch: " + e); }
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
