// InputBridge.cs. Lets Rust register per-keypress callbacks
// that fire from the shim's MonoBehaviour.Update on the Unity
// main thread.
//
// Shared between the Mono and IL2CPP shims. Both call
// `InputBridge.PollAll()` from their per-frame tick.

using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using UnityEngine;

namespace Unityforge.Shim
{
    public static class InputBridge
    {
        public delegate int RegisterFn(int keycode, IntPtr callbackFnPtr);
        public delegate void UnregisterFn(int binding);
        public delegate void KeyDownCallback();

        public static readonly RegisterFn RegisterDelegate = Register;
        public static readonly UnregisterFn UnregisterDelegate = Unregister;

        private struct Binding
        {
            public int Handle;
            public KeyCode Key;
            public IntPtr CallbackPtr;
        }

        private static readonly object _lock = new object();
        private static readonly List<Binding> _bindings = new List<Binding>();
        private static int _nextHandle = 1;

        private static int Register(int keycode, IntPtr callbackFnPtr)
        {
            if (callbackFnPtr == IntPtr.Zero) return 0;
            lock (_lock)
            {
                int h = _nextHandle++;
                _bindings.Add(new Binding
                {
                    Handle = h,
                    Key = (KeyCode)keycode,
                    CallbackPtr = callbackFnPtr,
                });
                return h;
            }
        }

        private static void Unregister(int binding)
        {
            if (binding == 0) return;
            lock (_lock)
            {
                for (int i = 0; i < _bindings.Count; i++)
                {
                    if (_bindings[i].Handle == binding)
                    {
                        _bindings.RemoveAt(i);
                        return;
                    }
                }
            }
        }

        /// <summary>
        /// Drop every binding. Used during hot reload so the
        /// next Update poll doesn't invoke a Rust callback in
        /// the freed DLL.
        /// </summary>
        public static void Clear()
        {
            lock (_lock) _bindings.Clear();
        }

        /// <summary>
        /// Called from the shim's MonoBehaviour.Update each frame.
        /// Polls `Input.GetKeyDown` for every registered binding
        /// and fires the matching Rust callback. The callback is
        /// invoked synchronously on the Unity main thread.
        /// </summary>
        public static void PollAll()
        {
            // Snapshot under the lock; fire outside so a callback
            // that calls Register/Unregister doesn't deadlock.
            Binding[] snapshot;
            lock (_lock)
            {
                snapshot = _bindings.ToArray();
            }
            for (int i = 0; i < snapshot.Length; i++)
            {
                try
                {
                    if (Input.GetKeyDown(snapshot[i].Key))
                    {
                        var cb = (KeyDownCallback)Marshal.GetDelegateForFunctionPointer(
                            snapshot[i].CallbackPtr, typeof(KeyDownCallback));
                        cb();
                    }
                }
                catch (Exception e)
                {
                    ShimLogger.Error(
                        "InputBridge: callback for key " + snapshot[i].Key + " threw: " + e);
                }
            }
        }
    }
}
