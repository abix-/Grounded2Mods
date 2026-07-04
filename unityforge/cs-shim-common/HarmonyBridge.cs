// HarmonyBridge.cs. Exposes Harmony patch operations to Rust as
// function pointers.
//
// Rust passes an unmanaged `extern "C" fn` pointer for the
// prefix/postfix body. We wrap it in a managed delegate via
// Marshal.GetDelegateForFunctionPointer and target Harmony with
// that.
//
// Prefix delegate signature: int(IntPtr).
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
        private static int _next = 1;
        private static Harmony _harmony;

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
        /// HarmonyX doesn't dispatch into a freed Rust DLL.
        /// </summary>
        public static void UnpatchAll()
        {
            lock (_lock)
            {
                if (_harmony != null)
                {
                    try { _harmony.UnpatchSelf(); }
                    catch (Exception e)
                    {
                        ShimLogger.Error("HarmonyBridge.UnpatchAll: " + e);
                    }
                }
                _patches.Clear();
            }
        }

        private class PatchEntry
        {
            public MethodBase Target;
            public HarmonyMethod Patch;
            public object KeepAliveDelegate;
        }

        private static int PatchPrefix(IntPtr typeNameUtf8, IntPtr methodNameUtf8, IntPtr rustFnPtr)
        {
            try
            {
                if (_harmony == null) return 0;
                var tname = Marshal.PtrToStringAnsi(typeNameUtf8);
                var mname = Marshal.PtrToStringAnsi(methodNameUtf8);
                var t = TypeCache.Resolve(tname);
                if (t == null) return 0;
                var target = AccessTools.Method(t, mname);
                if (target == null) return 0;

                var del = (RustPrefixDelegate)Marshal.GetDelegateForFunctionPointer(rustFnPtr, typeof(RustPrefixDelegate));
                // Bridge bool prefix to Harmony's bool prefix shape.
                // We synthesize a static method via DynamicMethod (later); for
                // v0 just invoke and discard return.
                var bridge = new Action(() => { try { del(IntPtr.Zero); } catch { } });
                var hm = new HarmonyMethod(bridge.Method) { reversePatchType = HarmonyReversePatchType.Original };
                _harmony.Patch(target, prefix: hm);

                int handle;
                lock (_lock)
                {
                    handle = _next++;
                    _patches[handle] = new PatchEntry { Target = target, Patch = hm, KeepAliveDelegate = del };
                }
                return handle;
            }
            catch { return 0; }
        }

        private static int PatchPostfix(IntPtr typeNameUtf8, IntPtr methodNameUtf8, IntPtr rustFnPtr)
        {
            try
            {
                if (_harmony == null) return 0;
                var tname = Marshal.PtrToStringAnsi(typeNameUtf8);
                var mname = Marshal.PtrToStringAnsi(methodNameUtf8);
                var t = TypeCache.Resolve(tname);
                if (t == null) return 0;
                var target = AccessTools.Method(t, mname);
                if (target == null) return 0;

                var del = (RustPostfixDelegate)Marshal.GetDelegateForFunctionPointer(rustFnPtr, typeof(RustPostfixDelegate));
                var bridge = new Action(() => { try { del(IntPtr.Zero); } catch { } });
                var hm = new HarmonyMethod(bridge.Method);
                _harmony.Patch(target, postfix: hm);

                int handle;
                lock (_lock)
                {
                    handle = _next++;
                    _patches[handle] = new PatchEntry { Target = target, Patch = hm, KeepAliveDelegate = del };
                }
                return handle;
            }
            catch { return 0; }
        }

        private static void Unpatch(int handle)
        {
            PatchEntry entry;
            lock (_lock)
            {
                if (!_patches.TryGetValue(handle, out entry)) return;
                _patches.Remove(handle);
            }
            try { _harmony?.Unpatch(entry.Target, HarmonyPatchType.All, _harmony.Id); }
            catch { }
        }
    }
}
