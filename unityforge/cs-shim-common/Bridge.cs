// Bridge.cs. The function-pointer table passed to Rust at init.
//
// Layout MUST match unityforge/src/bridge.rs `BridgeTable`
// exactly. Add fields at the END only; never reorder. Bump
// BRIDGE_VERSION on the Rust side AND here in lockstep.
//
// Linked into both cs-shim-mono and cs-shim-il2cpp. Each shim
// populates the same struct; the field names are backend-
// neutral. The shim tags `RuntimeKind` so Rust code that must
// branch on the backend can.

using System;
using System.Runtime.InteropServices;

namespace Unityforge.Shim
{
    public static class BridgeConstants
    {
        public const uint Magic = 0x52424655u; // "UFBR"
        public const uint Version = 5u;        // bumped: added HarmonyPatchPrefixCtx
    }

    public enum RuntimeKind : uint
    {
        Mono = 0,
        Il2Cpp = 1,
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct BridgeTable
    {
        public uint Magic;
        public uint Version;
        public uint RuntimeKind;

        // logging
        public IntPtr LogEmit;

        // reflection (backend-neutral; populated by the active shim)
        public IntPtr FindType;
        public IntPtr SingletonInstance;
        public IntPtr StaticInstance;
        public IntPtr WalkClass;
        public IntPtr InspectObject;
        public IntPtr ReadField;
        public IntPtr WriteField;
        public IntPtr InvokeMethod;
        public IntPtr ReleaseHandle;

        // harmony (HarmonyX works on both backends)
        public IntPtr HarmonyPatchPrefix;
        public IntPtr HarmonyPatchPostfix;
        public IntPtr HarmonyUnpatch;

        // input (v3+; shared, populated from InputBridge)
        public IntPtr RegisterKeyBinding;
        public IntPtr UnregisterKeyBinding;

        // reflection (v4+; method-list helper)
        public IntPtr ListMethods;

        // harmony (v5+; prefix that passes a context object handle
        // to the Rust callback: __instance or args[0])
        public IntPtr HarmonyPatchPrefixCtx;
    }

    /// <summary>
    /// The active shim implements this to expose its backend's
    /// reflection delegates. HarmonyBridge + Logger are shared
    /// across backends and don't go through this interface.
    /// </summary>
    public interface IBackendBridge
    {
        RuntimeKind Kind { get; }
        IntPtr FindType { get; }
        IntPtr SingletonInstance { get; }
        IntPtr StaticInstance { get; }
        IntPtr WalkClass { get; }
        IntPtr InspectObject { get; }
        IntPtr ReadField { get; }
        IntPtr WriteField { get; }
        IntPtr InvokeMethod { get; }
        IntPtr ReleaseHandle { get; }
        IntPtr ListMethods { get; }
    }

    public static class Bridge
    {
        public static BridgeTable Build(IBackendBridge backend)
        {
            return new BridgeTable
            {
                Magic = BridgeConstants.Magic,
                Version = BridgeConstants.Version,
                RuntimeKind = (uint)backend.Kind,

                LogEmit = Marshal.GetFunctionPointerForDelegate(ShimLogger.EmitDelegate),

                FindType = backend.FindType,
                SingletonInstance = backend.SingletonInstance,
                StaticInstance = backend.StaticInstance,
                WalkClass = backend.WalkClass,
                InspectObject = backend.InspectObject,
                ReadField = backend.ReadField,
                WriteField = backend.WriteField,
                InvokeMethod = backend.InvokeMethod,
                ReleaseHandle = backend.ReleaseHandle,

                HarmonyPatchPrefix = Marshal.GetFunctionPointerForDelegate(HarmonyBridge.PatchPrefixDelegate),
                HarmonyPatchPostfix = Marshal.GetFunctionPointerForDelegate(HarmonyBridge.PatchPostfixDelegate),
                HarmonyUnpatch = Marshal.GetFunctionPointerForDelegate(HarmonyBridge.UnpatchDelegate),

                RegisterKeyBinding = Marshal.GetFunctionPointerForDelegate(InputBridge.RegisterDelegate),
                UnregisterKeyBinding = Marshal.GetFunctionPointerForDelegate(InputBridge.UnregisterDelegate),

                ListMethods = backend.ListMethods,

                HarmonyPatchPrefixCtx = Marshal.GetFunctionPointerForDelegate(HarmonyBridge.PatchPrefixCtxDelegate),
            };
        }
    }
}
