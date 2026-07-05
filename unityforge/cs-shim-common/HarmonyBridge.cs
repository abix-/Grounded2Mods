// HarmonyBridge.cs. Exposes Harmony patch operations to Rust as
// function pointers.
//
// Rust passes an unmanaged `extern "C" fn` pointer for the
// prefix/postfix body. We wrap it in a managed delegate via
// Marshal.GetDelegateForFunctionPointer and route each call
// through a PRE-COMPILED STATIC SLOT METHOD (one slot per live
// patch).
//
// Why slots (iteration history, all live-verified 2026-07-04 on
// Survivalist: Invisible Strain, Unity 6000 Mono + official
// pardeike Harmony 2.0.4):
//   1. `new Action(() => del(...)).Method` as the Harmony target
//      is an instance method on a closure class; HarmonyLib
//      rejects it. Every Rust patch silently failed.
//   2. One shared static dispatcher routed by `MethodBase
//      __originalMethod` compiles, but Harmony 2.0.4 emits
//      `Ldtoken original` + `Call MethodBase.GetMethodFromHandle`
//      for that parameter (MethodPatcher.cs at tag v2.0.4.0), and
//      the game's Mono cannot resolve that call token inside the
//      dynamic wrapper: "Invalid IL code in (wrapper
//      dynamic-method) ... IL_001e: call 0x00000005".
//   3. `object[] __args` does not exist in 2.0.4 at all (parses
//      as an invalid indexed parameter).
// The slot signatures below use ONLY parameter emissions that are
// plain `ldarg` loads with zero metadata tokens (verified against
// MethodPatcher.cs v2.0.4.0): no parameters, `object __instance`,
// or `object __0`. That is the same shape the game's working mods
// (DisableHUD, SISLootRespawn) use.
//
// Patch kinds:
//   - prefix:  Rust int(IntPtr). Non-zero return = skip the
//     original (matches unityforge/src/hook.rs). ctx = 0.
//   - postfix: Rust void(IntPtr). ctx = 0.
//   - prefix_ctx (bridge v5): Rust int(IntPtr) where the pointer
//     carries a FRESH object handle: ctxKind 0 = __instance
//     (instance methods only), ctxKind 1 = args[0]
//     (REFERENCE-type first argument only; indexed args are
//     loaded as-is with no boxing, so a value-type first arg
//     would produce invalid IL). The Rust callback OWNS the
//     handle and must release it (MonoObject::from_handle +
//     Drop). Zero when the context object is null.

using System;
using System.Collections.Generic;
using System.Reflection;
using System.Runtime.InteropServices;
using HarmonyLib;

namespace Unityforge.Shim
{
    public static class HarmonyBridge
    {
        private const int SlotsPerKind = 16;

        private static readonly object _lock = new object();
        private static readonly Dictionary<int, PatchEntry> _patches = new Dictionary<int, PatchEntry>();
        private static int _next = 1;
        private static Harmony _harmony;

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
        /// Per-slot unpatch, not UnpatchSelf: UnpatchSelf is
        /// HarmonyX-only (missing in pardeike Harmony 2.0.4).
        /// </summary>
        public static void UnpatchAll()
        {
            lock (_lock)
            {
                foreach (var kv in _patches)
                {
                    ReleaseEntry(kv.Value);
                }
                _patches.Clear();
            }
        }

        private enum PatchKind
        {
            Prefix,
            Postfix,
            PrefixInstanceCtx,
            PrefixArg0Ctx,
            // args[0] via Harmony's __args array: the ONLY variant
            // whose mutations reach a VALUE-TYPE argument. Harmony
            // documents that editing __args elements writes back to
            // the original arguments after the patch; mutating the
            // boxed element's fields therefore lands in the real
            // arg. Requires __args support (Harmony 2.1+; the
            // survivalist shim embeds 2.4.2). Plain Arg0Ctx hands a
            // boxed COPY for value types: writes are silently lost
            // (live-verified 2026-07-04: Injury is a struct, the
            // AddInjury infection zeroing did nothing in play).
            PrefixArgs0Ctx,
        }

        private class PatchEntry
        {
            public MethodBase Target;
            public PatchKind Kind;
            public int Slot;
        }

        // ---- slot tables -------------------------------------------------
        // One delegate per live patch. The pre-compiled slot
        // methods below read their table entry and call the Rust
        // fn. A null entry (raced unpatch) is a no-op.

        private static readonly RustPrefixDelegate[] _prefixSlots = new RustPrefixDelegate[SlotsPerKind];
        private static readonly RustPostfixDelegate[] _postfixSlots = new RustPostfixDelegate[SlotsPerKind];
        private static readonly RustPrefixDelegate[] _prefixInstanceSlots = new RustPrefixDelegate[SlotsPerKind];
        private static readonly RustPrefixDelegate[] _prefixArg0Slots = new RustPrefixDelegate[SlotsPerKind];
        private static readonly RustPrefixDelegate[] _prefixArgs0Slots = new RustPrefixDelegate[SlotsPerKind];

        private static bool RunPrefixSlot(int i)
        {
            var d = _prefixSlots[i];
            if (d == null) return true;
            try
            {
                // Non-zero from the Rust prefix = skip the original
                // (unityforge/src/hook.rs contract).
                return d(IntPtr.Zero) == 0;
            }
            catch (Exception e)
            {
                ShimLogger.Error("HarmonyBridge: prefix slot " + i + " threw: " + e);
                return true;
            }
        }

        private static void RunPostfixSlot(int i)
        {
            var d = _postfixSlots[i];
            if (d == null) return;
            try { d(IntPtr.Zero); }
            catch (Exception e)
            {
                ShimLogger.Error("HarmonyBridge: postfix slot " + i + " threw: " + e);
            }
        }

        private static bool RunPrefixCtxSlot(RustPrefixDelegate[] table, int i, object ctx)
        {
            var d = table[i];
            if (d == null) return true;
            // A FRESH handle per call: the Rust side owns it and
            // releases it (MonoObject Drop).
            var handle = (ctx != null) ? MonoBridge.Acquire(ctx) : 0;
            try
            {
                return d(new IntPtr(handle)) == 0;
            }
            catch (Exception e)
            {
                ShimLogger.Error("HarmonyBridge: prefix_ctx slot " + i + " threw: " + e);
                return true;
            }
        }

        // ---- pre-compiled slot methods ------------------------------------
        // These are the methods Harmony targets. Signatures use
        // ONLY token-free parameter emissions (see header).

        private static bool PrefixSlot0() => RunPrefixSlot(0);
        private static bool PrefixSlot1() => RunPrefixSlot(1);
        private static bool PrefixSlot2() => RunPrefixSlot(2);
        private static bool PrefixSlot3() => RunPrefixSlot(3);
        private static bool PrefixSlot4() => RunPrefixSlot(4);
        private static bool PrefixSlot5() => RunPrefixSlot(5);
        private static bool PrefixSlot6() => RunPrefixSlot(6);
        private static bool PrefixSlot7() => RunPrefixSlot(7);
        private static bool PrefixSlot8() => RunPrefixSlot(8);
        private static bool PrefixSlot9() => RunPrefixSlot(9);
        private static bool PrefixSlot10() => RunPrefixSlot(10);
        private static bool PrefixSlot11() => RunPrefixSlot(11);
        private static bool PrefixSlot12() => RunPrefixSlot(12);
        private static bool PrefixSlot13() => RunPrefixSlot(13);
        private static bool PrefixSlot14() => RunPrefixSlot(14);
        private static bool PrefixSlot15() => RunPrefixSlot(15);

        private static void PostfixSlot0() => RunPostfixSlot(0);
        private static void PostfixSlot1() => RunPostfixSlot(1);
        private static void PostfixSlot2() => RunPostfixSlot(2);
        private static void PostfixSlot3() => RunPostfixSlot(3);
        private static void PostfixSlot4() => RunPostfixSlot(4);
        private static void PostfixSlot5() => RunPostfixSlot(5);
        private static void PostfixSlot6() => RunPostfixSlot(6);
        private static void PostfixSlot7() => RunPostfixSlot(7);
        private static void PostfixSlot8() => RunPostfixSlot(8);
        private static void PostfixSlot9() => RunPostfixSlot(9);
        private static void PostfixSlot10() => RunPostfixSlot(10);
        private static void PostfixSlot11() => RunPostfixSlot(11);
        private static void PostfixSlot12() => RunPostfixSlot(12);
        private static void PostfixSlot13() => RunPostfixSlot(13);
        private static void PostfixSlot14() => RunPostfixSlot(14);
        private static void PostfixSlot15() => RunPostfixSlot(15);

        private static bool PrefixInstanceSlot0(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 0, __instance);
        private static bool PrefixInstanceSlot1(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 1, __instance);
        private static bool PrefixInstanceSlot2(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 2, __instance);
        private static bool PrefixInstanceSlot3(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 3, __instance);
        private static bool PrefixInstanceSlot4(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 4, __instance);
        private static bool PrefixInstanceSlot5(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 5, __instance);
        private static bool PrefixInstanceSlot6(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 6, __instance);
        private static bool PrefixInstanceSlot7(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 7, __instance);
        private static bool PrefixInstanceSlot8(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 8, __instance);
        private static bool PrefixInstanceSlot9(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 9, __instance);
        private static bool PrefixInstanceSlot10(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 10, __instance);
        private static bool PrefixInstanceSlot11(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 11, __instance);
        private static bool PrefixInstanceSlot12(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 12, __instance);
        private static bool PrefixInstanceSlot13(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 13, __instance);
        private static bool PrefixInstanceSlot14(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 14, __instance);
        private static bool PrefixInstanceSlot15(object __instance) => RunPrefixCtxSlot(_prefixInstanceSlots, 15, __instance);

        private static object Args0(object[] __args)
            => (__args != null && __args.Length > 0) ? __args[0] : null;

        private static bool PrefixArgs0Slot0(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 0, Args0(__args));
        private static bool PrefixArgs0Slot1(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 1, Args0(__args));
        private static bool PrefixArgs0Slot2(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 2, Args0(__args));
        private static bool PrefixArgs0Slot3(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 3, Args0(__args));
        private static bool PrefixArgs0Slot4(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 4, Args0(__args));
        private static bool PrefixArgs0Slot5(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 5, Args0(__args));
        private static bool PrefixArgs0Slot6(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 6, Args0(__args));
        private static bool PrefixArgs0Slot7(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 7, Args0(__args));
        private static bool PrefixArgs0Slot8(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 8, Args0(__args));
        private static bool PrefixArgs0Slot9(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 9, Args0(__args));
        private static bool PrefixArgs0Slot10(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 10, Args0(__args));
        private static bool PrefixArgs0Slot11(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 11, Args0(__args));
        private static bool PrefixArgs0Slot12(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 12, Args0(__args));
        private static bool PrefixArgs0Slot13(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 13, Args0(__args));
        private static bool PrefixArgs0Slot14(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 14, Args0(__args));
        private static bool PrefixArgs0Slot15(object[] __args) => RunPrefixCtxSlot(_prefixArgs0Slots, 15, Args0(__args));

        private static bool PrefixArg0Slot0(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 0, __0);
        private static bool PrefixArg0Slot1(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 1, __0);
        private static bool PrefixArg0Slot2(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 2, __0);
        private static bool PrefixArg0Slot3(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 3, __0);
        private static bool PrefixArg0Slot4(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 4, __0);
        private static bool PrefixArg0Slot5(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 5, __0);
        private static bool PrefixArg0Slot6(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 6, __0);
        private static bool PrefixArg0Slot7(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 7, __0);
        private static bool PrefixArg0Slot8(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 8, __0);
        private static bool PrefixArg0Slot9(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 9, __0);
        private static bool PrefixArg0Slot10(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 10, __0);
        private static bool PrefixArg0Slot11(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 11, __0);
        private static bool PrefixArg0Slot12(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 12, __0);
        private static bool PrefixArg0Slot13(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 13, __0);
        private static bool PrefixArg0Slot14(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 14, __0);
        private static bool PrefixArg0Slot15(object __0) => RunPrefixCtxSlot(_prefixArg0Slots, 15, __0);

        private static MethodInfo SlotMi(string prefix, int i)
        {
            return typeof(HarmonyBridge).GetMethod(prefix + i, BindingFlags.NonPublic | BindingFlags.Static);
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
                return ApplySlotPatch(target, PatchKind.Prefix, del, null);
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
                return ApplySlotPatch(target, PatchKind.Postfix, null, del);
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
                if (ctxKind != 0 && ctxKind != 1 && ctxKind != 2) return 0;
                var target = ResolveTarget(typeNameUtf8, methodNameUtf8);
                if (target == null) return 0;
                if (ctxKind == 0 && target.IsStatic)
                {
                    ShimLogger.Error("HarmonyBridge.PatchPrefixCtx: __instance ctx on static method " + target.Name);
                    return 0;
                }
                if (ctxKind != 0 && target.GetParameters().Length == 0)
                {
                    ShimLogger.Error("HarmonyBridge.PatchPrefixCtx: arg ctx on parameterless method " + target.Name);
                    return 0;
                }
                if (ctxKind == 1 && target.GetParameters()[0].ParameterType.IsValueType)
                {
                    // A boxed COPY would be handed to the callback and
                    // every mutation silently lost. Force the caller to
                    // the __args write-back variant.
                    ShimLogger.Error("HarmonyBridge.PatchPrefixCtx: arg0 ctx on VALUE-TYPE first arg of " + target.Name + "; use ctx kind 2 (args0 write-back)");
                    return 0;
                }
                var del = (RustPrefixDelegate)Marshal.GetDelegateForFunctionPointer(rustFnPtr, typeof(RustPrefixDelegate));
                var kind = (ctxKind == 0) ? PatchKind.PrefixInstanceCtx
                    : (ctxKind == 1) ? PatchKind.PrefixArg0Ctx
                    : PatchKind.PrefixArgs0Ctx;
                return ApplySlotPatch(target, kind, del, null);
            }
            catch (Exception e)
            {
                ShimLogger.Error("HarmonyBridge.PatchPrefixCtx: " + e);
                return 0;
            }
        }

        private static int ApplySlotPatch(MethodBase target, PatchKind kind, RustPrefixDelegate prefixDel, RustPostfixDelegate postfixDel)
        {
            lock (_lock)
            {
                string namePrefix = SlotNamePrefix(kind);

                int slot = FindFreeSlot(kind);
                if (slot < 0)
                {
                    ShimLogger.Error($"HarmonyBridge: no free {namePrefix} (cap {SlotsPerKind}); unpatch something or raise SlotsPerKind");
                    return 0;
                }

                var mi = SlotMi(namePrefix, slot);
                var hm = new HarmonyMethod(mi);
                // Assign the delegate BEFORE patching so the slot
                // is live the instant the patch applies; clear on
                // failure.
                SetSlot(kind, slot, prefixDel, postfixDel);
                try
                {
                    if (kind == PatchKind.Postfix) _harmony.Patch(target, postfix: hm);
                    else _harmony.Patch(target, prefix: hm);
                }
                catch
                {
                    SetSlot(kind, slot, null, null);
                    throw;
                }

                int handle = _next++;
                _patches[handle] = new PatchEntry { Target = target, Kind = kind, Slot = slot };
                return handle;
            }
        }

        private static string SlotNamePrefix(PatchKind kind)
        {
            switch (kind)
            {
                case PatchKind.Prefix: return "PrefixSlot";
                case PatchKind.Postfix: return "PostfixSlot";
                case PatchKind.PrefixInstanceCtx: return "PrefixInstanceSlot";
                case PatchKind.PrefixArg0Ctx: return "PrefixArg0Slot";
                default: return "PrefixArgs0Slot";
            }
        }

        private static int FindFreeSlot(PatchKind kind)
        {
            for (int i = 0; i < SlotsPerKind; i++)
            {
                bool free;
                switch (kind)
                {
                    case PatchKind.Prefix: free = _prefixSlots[i] == null; break;
                    case PatchKind.Postfix: free = _postfixSlots[i] == null; break;
                    case PatchKind.PrefixInstanceCtx: free = _prefixInstanceSlots[i] == null; break;
                    case PatchKind.PrefixArg0Ctx: free = _prefixArg0Slots[i] == null; break;
                    default: free = _prefixArgs0Slots[i] == null; break;
                }
                if (free) return i;
            }
            return -1;
        }

        private static void SetSlot(PatchKind kind, int slot, RustPrefixDelegate prefixDel, RustPostfixDelegate postfixDel)
        {
            switch (kind)
            {
                case PatchKind.Prefix: _prefixSlots[slot] = prefixDel; break;
                case PatchKind.Postfix: _postfixSlots[slot] = postfixDel; break;
                case PatchKind.PrefixInstanceCtx: _prefixInstanceSlots[slot] = prefixDel; break;
                case PatchKind.PrefixArg0Ctx: _prefixArg0Slots[slot] = prefixDel; break;
                default: _prefixArgs0Slots[slot] = prefixDel; break;
            }
        }

        private static void Unpatch(int handle)
        {
            lock (_lock)
            {
                if (!_patches.TryGetValue(handle, out var entry)) return;
                _patches.Remove(handle);
                ReleaseEntry(entry);
            }
        }

        private static void ReleaseEntry(PatchEntry entry)
        {
            string namePrefix = SlotNamePrefix(entry.Kind);
            try { _harmony?.Unpatch(entry.Target, SlotMi(namePrefix, entry.Slot)); }
            catch (Exception e) { ShimLogger.Error("HarmonyBridge.Unpatch: " + e); }
            SetSlot(entry.Kind, entry.Slot, null, null);
        }

        private static MethodBase ResolveTarget(IntPtr typeNameUtf8, IntPtr methodNameUtf8)
        {
            var tname = Marshal.PtrToStringAnsi(typeNameUtf8);
            var mname = Marshal.PtrToStringAnsi(methodNameUtf8);
            var t = TypeCache.Resolve(tname);
            if (t == null)
            {
                // Loud on the miss paths: a silent 0 here cost a
                // full game-restart debug cycle (2026-07-04).
                ShimLogger.Error($"HarmonyBridge: type '{tname}' not found");
                return null;
            }
            var m = AccessTools.Method(t, mname);
            if (m == null)
            {
                ShimLogger.Error($"HarmonyBridge: method '{mname}' not found on {t.FullName} (assembly {t.Assembly.GetName().Name})");
            }
            return m;
        }
    }
}
