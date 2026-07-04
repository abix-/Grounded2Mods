// Logger.cs. Host-agnostic log seam. The host sets `Sink` at
// startup (BepInEx hosts route to their ManualLogSource; a
// game-official-loader host routes to UnityEngine.Debug.Log).
// Rust log lines arrive through EmitDelegate and go to the same
// sink.
//
// Levels: 0/1 debug, 2 info, 3 warning, 4 error.

using System;
using System.Runtime.InteropServices;

namespace Unityforge.Shim
{
    public static class ShimLogger
    {
        public delegate void SinkFn(int level, string msg);

        public static SinkFn Sink;

        public static void Debug(string msg) => Sink?.Invoke(1, msg);
        public static void Info(string msg) => Sink?.Invoke(2, msg);
        public static void Warn(string msg) => Sink?.Invoke(3, msg);
        public static void Error(string msg) => Sink?.Invoke(4, msg);

        public delegate void EmitFn(int level, IntPtr msgUtf8);

        // The delegate field must be static so the GC doesn't
        // collect it before Rust calls back.
        public static readonly EmitFn EmitDelegate = Emit;

        private static void Emit(int level, IntPtr msgUtf8)
        {
            var sink = Sink;
            if (sink == null) return;
            var msg = Marshal.PtrToStringAnsi(msgUtf8) ?? "<null>";
            sink(level, msg);
        }
    }
}
