// Il2CppBridge.cs. Reflection helpers exposed to Rust as
// function pointers, IL2CPP variant.
//
// Surface mirrors MonoBridge.cs (Mono variant). The Rust side
// is backend-agnostic: it sees neutral bridge entries
// (FindType, SingletonInstance, etc.) and calls them by
// function pointer. Whichever shim is loaded fills the slots
// with its backend's implementation.
//
// IL2CPP semantics:
// - `Il2CppType.From(name)` resolves a System.Type that wraps
//   an Il2Cpp class. The same `name` string format Mono uses
//   (e.g. "UnityEngine.Time") works.
// - Native fields surface on the generated proxy types as
//   managed PROPERTIES (backed by NativeFieldInfoPtr), not as
//   managed fields. Field reads/writes therefore consult
//   GetField first, then GetProperty. Found live 2026-08-07:
//   GetField alone saw only wrapper internals
//   (isWrapped/pooledPtr) on ScheduleOne.Map.Map.
// - Resources.FindObjectsOfTypeAll returns UnityEngine.Object
//   wrappers; each is downcast to the requested proxy type via
//   the proxy's (IntPtr) ctor so reflection sees the game
//   class, not the wrapper.
// - HarmonyX targeting Il2Cpp methods works the same way as
//   on Mono once the type is loaded (HarmonyX abstracts it).
//
// Newtonsoft.Json is the JSON shape on out-buffers, same as
// the Mono shim. The byte-level wire format is identical so
// Rust deserializes the same way regardless of backend.

using System;
using System.Collections.Generic;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using Il2CppInterop.Runtime;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

namespace Unityforge.Shim
{
    /// <summary>
    /// IBackendBridge adapter so Bridge.Build can populate the
    /// neutral-named struct from Il2CppBridge's delegates.
    /// </summary>
    public sealed class Il2CppBackendBridge : IBackendBridge
    {
        public RuntimeKind Kind => RuntimeKind.Il2Cpp;
        public IntPtr FindType => Marshal.GetFunctionPointerForDelegate(Il2CppBridge.FindTypeDelegate);
        public IntPtr SingletonInstance => Marshal.GetFunctionPointerForDelegate(Il2CppBridge.SingletonInstanceDelegate);
        public IntPtr StaticInstance => Marshal.GetFunctionPointerForDelegate(Il2CppBridge.StaticInstanceDelegate);
        public IntPtr WalkClass => Marshal.GetFunctionPointerForDelegate(Il2CppBridge.WalkClassDelegate);
        public IntPtr InspectObject => Marshal.GetFunctionPointerForDelegate(Il2CppBridge.InspectObjectDelegate);
        public IntPtr ReadField => Marshal.GetFunctionPointerForDelegate(Il2CppBridge.ReadFieldDelegate);
        public IntPtr WriteField => Marshal.GetFunctionPointerForDelegate(Il2CppBridge.WriteFieldDelegate);
        public IntPtr InvokeMethod => Marshal.GetFunctionPointerForDelegate(Il2CppBridge.InvokeMethodDelegate);
        public IntPtr InvokeStatic => Marshal.GetFunctionPointerForDelegate(Il2CppBridge.InvokeStaticDelegate);
        public IntPtr ReleaseHandle => Marshal.GetFunctionPointerForDelegate(Il2CppBridge.ReleaseHandleDelegate);
        public IntPtr ListMethods => Marshal.GetFunctionPointerForDelegate(Il2CppBridge.ListMethodsDelegate);
    }

    public static class Il2CppBridge
    {
        // ---- handle table ----------------------------------------------

        private static readonly object _lock = new object();
        private static readonly Dictionary<int, object> _handles = new Dictionary<int, object>();
        private static int _next = 1;

        public static int Acquire(object obj)
        {
            if (obj == null) return 0;
            lock (_lock)
            {
                int h = _next++;
                _handles[h] = obj;
                return h;
            }
        }

        public static object Lookup(int handle)
        {
            if (handle == 0) return null;
            lock (_lock)
            {
                _handles.TryGetValue(handle, out var v);
                return v;
            }
        }

        /// <summary>
        /// Drop every cached handle. Used during hot reload.
        /// </summary>
        public static void ClearHandles()
        {
            lock (_lock) { _handles.Clear(); _next = 1; }
        }

        // ---- delegate types --------------------------------------------

        public delegate int FindTypeFn(IntPtr nameUtf8);
        public delegate int SingletonInstanceFn(int typeHandle);
        public delegate int StaticInstanceFn(int typeHandle);
        public delegate int WalkClassFn(int typeHandle, int includeInactive, IntPtr outBuf, int cap);
        public delegate int InspectObjectFn(int handle, IntPtr outBuf, int cap);
        public delegate int ReadFieldFn(int handle, IntPtr fieldNameUtf8, IntPtr outBuf, int cap);
        public delegate int WriteFieldFn(int handle, IntPtr fieldNameUtf8, IntPtr valueJsonUtf8);
        public delegate int InvokeMethodFn(int handle, IntPtr methodNameUtf8, IntPtr argsJsonUtf8, IntPtr outBuf, int cap);
        public delegate void ReleaseHandleFn(int handle);

        public static readonly FindTypeFn FindTypeDelegate = FindType;
        public static readonly SingletonInstanceFn SingletonInstanceDelegate = SingletonInstance;
        public static readonly StaticInstanceFn StaticInstanceDelegate = StaticInstance;
        public static readonly WalkClassFn WalkClassDelegate = WalkClass;
        public static readonly InspectObjectFn InspectObjectDelegate = InspectObject;
        public static readonly ReadFieldFn ReadFieldDelegate = ReadField;
        public static readonly WriteFieldFn WriteFieldDelegate = WriteField;
        public static readonly InvokeMethodFn InvokeMethodDelegate = InvokeMethod;
        public delegate int InvokeStaticFn(IntPtr classNameUtf8, IntPtr methodNameUtf8, IntPtr argsJsonUtf8, IntPtr outBuf, int cap);
        public static readonly InvokeStaticFn InvokeStaticDelegate = InvokeStatic;
        private static int InvokeStatic(IntPtr classNameUtf8, IntPtr methodNameUtf8, IntPtr argsJsonUtf8, IntPtr outBuf, int cap)
        {
            return -1; // static invoke is not implemented on the IL2CPP backend yet
        }
        public static readonly ReleaseHandleFn ReleaseHandleDelegate = ReleaseHandle;
        public delegate int ListMethodsFn(IntPtr typeNameUtf8, IntPtr outBuf, int cap);
        public static readonly ListMethodsFn ListMethodsDelegate = ListMethods;

        private static int ListMethods(IntPtr typeNameUtf8, IntPtr outBuf, int cap)
        {
            // Body shared with the Mono backend; see
            // TypeCache.ListMethods (cs-shim-common). On IL2CPP the
            // reflection walk runs over the Il2CppInterop proxy
            // types, which is what Harmony patches target here.
            var name = Marshal.PtrToStringAnsi(typeNameUtf8);
            return WriteJsonToBuf(TypeCache.ListMethods(name).ToString(Formatting.None), outBuf, cap);
        }

        // ---- implementations -------------------------------------------

        private static int FindType(IntPtr nameUtf8)
        {
            var name = Marshal.PtrToStringAnsi(nameUtf8);
            if (string.IsNullOrEmpty(name)) return 0;
            // Resolve against the loaded Il2CppInterop proxy
            // assemblies via the shared TypeCache (exact match
            // first, short-name scan second). The previous
            // `Il2CppType.From(name, throwOnError:)` call never
            // compiled: Il2CppType.From takes a System.Type, not a
            // string (found 2026-08-07 on the first real build of
            // this backend). Proxy types are what Harmony patches
            // and reflection reads target here, so System.Type is
            // the right currency for handles.
            try
            {
                var t = TypeCache.Resolve(name);
                if (t == null)
                {
                    ShimLogger.Error($"Il2CppBridge: type '{name}' not found");
                    return 0;
                }
                return Acquire(t);
            }
            catch
            {
                return 0;
            }
        }

        private static int SingletonInstance(int typeHandle)
        {
            var t = Lookup(typeHandle) as Type;
            if (t == null) return 0;
            var instance = ResolveSingleton(t);
            return Acquire(instance);
        }

        private static int StaticInstance(int typeHandle)
        {
            var t = Lookup(typeHandle) as Type;
            if (t == null) return 0;
            // StaticInstance<T>.Instance pattern. The custom static-
            // instance class names this `Instance` getter; look it up
            // on the bound type.
            var prop = t.GetProperty("Instance", BindingFlags.Public | BindingFlags.Static);
            var v = prop?.GetValue(null, null);
            if (v != null) return Acquire(v);
            var field = t.GetField("Instance", BindingFlags.Public | BindingFlags.Static);
            return Acquire(field?.GetValue(null));
        }

        private static int WalkClass(int typeHandle, int includeInactive, IntPtr outBuf, int cap)
        {
            var t = Lookup(typeHandle) as Type;
            if (t == null) return -1;
            // Object.FindObjectsOfTypeAll equivalent on IL2CPP comes
            // via Resources.FindObjectsOfTypeAll, which takes an
            // Il2CppSystem.Type; Il2CppType.From converts from the
            // proxy System.Type. Caller must ensure t derives from
            // UnityEngine.Object for this path to return anything.
            var arr = UnityEngine.Resources.FindObjectsOfTypeAll(Il2CppType.From(t));
            var list = new JArray();
            for (int i = 0; arr != null && i < arr.Count; i++)
            {
                var o = arr[i];
                if (o == null) continue;
                int h = Acquire(DowncastToProxy(t, o));
                list.Add(new JObject { ["handle"] = h, ["name"] = o.name?.ToString() ?? "" });
            }
            return WriteJsonToBuf(list.ToString(Formatting.None), outBuf, cap);
        }

        private static int InspectObject(int handle, IntPtr outBuf, int cap)
        {
            var obj = Lookup(handle);
            if (obj == null) return -1;
            var t = obj.GetType();
            var root = new JObject { ["type"] = t.FullName };
            var fields = new JObject();
            foreach (var f in t.GetFields(BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance))
            {
                try
                {
                    var v = f.GetValue(obj);
                    fields[f.Name] = JToken.FromObject(SerializeSafe(v) ?? JValue.CreateNull());
                }
                catch (Exception e)
                {
                    fields[f.Name] = "<error: " + e.Message + ">";
                }
            }
            // Native fields surface as properties on the proxy
            // type; wrapper plumbing declared on Il2CppObjectBase
            // itself is skipped. Only FIELD-backed properties
            // (static NativeFieldInfoPtr_<name> on the declaring
            // type) are read: those are memory reads. Method-backed
            // getters run arbitrary game code and crashed the game
            // when blanket-invoked (0xc0000005, 2026-08-07); they
            // are listed as markers and read explicitly via
            // read_field / invoke_method instead.
            foreach (var p in t.GetProperties(BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance))
            {
                if (!p.CanRead || p.GetIndexParameters().Length > 0) continue;
                if (p.DeclaringType == typeof(Il2CppInterop.Runtime.InteropTypes.Il2CppObjectBase)) continue;
                if (!IsFieldBackedProperty(p))
                {
                    fields[p.Name] = "<getter: " + p.PropertyType.Name + ">";
                    continue;
                }
                try
                {
                    var v = p.GetValue(obj);
                    fields[p.Name] = JToken.FromObject(SerializeSafe(v) ?? JValue.CreateNull());
                }
                catch (Exception e)
                {
                    fields[p.Name] = "<error: " + e.Message + ">";
                }
            }
            root["fields"] = fields;
            return WriteJsonToBuf(root.ToString(Formatting.None), outBuf, cap);
        }

        private static int ReadField(int handle, IntPtr fieldNameUtf8, IntPtr outBuf, int cap)
        {
            var obj = Lookup(handle);
            if (obj == null) return -1;
            var name = Marshal.PtrToStringAnsi(fieldNameUtf8);
            if (string.IsNullOrEmpty(name)) return -1;
            var t = obj.GetType();
            var f = t.GetField(name, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
            try
            {
                if (f != null)
                {
                    var v = f.GetValue(obj);
                    return WriteJsonToBuf(JsonConvert.SerializeObject(SerializeSafe(v)), outBuf, cap);
                }
                // Native fields surface as properties on the proxy.
                var p = t.GetProperty(name, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
                if (p == null || !p.CanRead) return -1;
                var pv = p.GetValue(obj);
                return WriteJsonToBuf(JsonConvert.SerializeObject(SerializeSafe(pv)), outBuf, cap);
            }
            catch
            {
                return -1;
            }
        }

        private static int WriteField(int handle, IntPtr fieldNameUtf8, IntPtr valueJsonUtf8)
        {
            var obj = Lookup(handle);
            if (obj == null) return -2;
            var name = Marshal.PtrToStringAnsi(fieldNameUtf8);
            if (string.IsNullOrEmpty(name)) return -2;
            var t = obj.GetType();
            var f = t.GetField(name, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
            var json = Marshal.PtrToStringAnsi(valueJsonUtf8) ?? "";
            try
            {
                if (f != null)
                {
                    var parsed = JsonConvert.DeserializeObject(json, f.FieldType);
                    f.SetValue(obj, parsed);
                    return 0;
                }
                // Native fields surface as properties on the proxy.
                var p = t.GetProperty(name, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
                if (p == null || !p.CanWrite) return -2;
                var pparsed = JsonConvert.DeserializeObject(json, p.PropertyType);
                p.SetValue(obj, pparsed);
                return 0;
            }
            catch
            {
                return -1;
            }
        }

        private static int InvokeMethod(int handle, IntPtr methodNameUtf8, IntPtr argsJsonUtf8, IntPtr outBuf, int cap)
        {
            var obj = Lookup(handle);
            if (obj == null) return -1;
            var name = Marshal.PtrToStringAnsi(methodNameUtf8);
            if (string.IsNullOrEmpty(name)) return -1;
            var argsJson = Marshal.PtrToStringAnsi(argsJsonUtf8) ?? "[]";
            JArray args;
            try { args = JArray.Parse(argsJson); }
            catch { return -2; }
            var t = obj.GetType();
            foreach (var m in t.GetMethods(BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance))
            {
                if (m.Name != name) continue;
                var pars = m.GetParameters();
                if (pars.Length != args.Count) continue;
                try
                {
                    var converted = new object[args.Count];
                    for (int i = 0; i < args.Count; i++)
                        converted[i] = ResolveArg(args[i], pars[i].ParameterType);
                    var result = m.Invoke(obj, converted);
                    var resultJson = JsonConvert.SerializeObject(SerializeSafe(result));
                    return WriteJsonToBuf(resultJson, outBuf, cap);
                }
                catch (Exception e)
                {
                    var err = "{\"error\":\"" + e.Message.Replace("\"", "\\\"") + "\"}";
                    WriteJsonToBuf(err, outBuf, cap);
                    return -3;
                }
            }
            return -1;
        }

        private static void ReleaseHandle(int handle)
        {
            if (handle == 0) return;
            lock (_lock) _handles.Remove(handle);
        }

        // ---- helpers ---------------------------------------------------

        private static int WriteJsonToBuf(string json, IntPtr outBuf, int cap)
        {
            var bytes = Encoding.UTF8.GetBytes(json);
            if (bytes.Length > cap) return -1;
            Marshal.Copy(bytes, 0, outBuf, bytes.Length);
            return bytes.Length;
        }

        /// <summary>
        /// JSON-friendly coercion. Il2Cpp values (Vector3, Color,
        /// boxed structs) often don't have a Newtonsoft converter
        /// off the shelf; fall back to ToString for those.
        /// </summary>
        /// <summary>
        /// Convert one invoke arg. {"$handle": N} resolves to the
        /// live object (HandleArg, shared); everything else goes
        /// through Newtonsoft. A resolved proxy that doesn't
        /// already satisfy the parameter type (e.g. Player into
        /// ICombatTargetable) is re-cast via the interop generic
        /// Cast&lt;T&gt;, since proxy interface tables live il2cpp-side.
        /// </summary>
        private static object ResolveArg(JToken tok, Type paramType)
        {
            if (!HandleArg.TryResolve(tok, Lookup, out var v))
                return tok.ToObject(paramType);
            if (v != null
                && !paramType.IsInstanceOfType(v)
                && v is Il2CppInterop.Runtime.InteropTypes.Il2CppObjectBase b)
            {
                try
                {
                    var cast = typeof(Il2CppInterop.Runtime.InteropTypes.Il2CppObjectBase)
                        .GetMethod("Cast")?.MakeGenericMethod(paramType);
                    if (cast != null) return cast.Invoke(b, null);
                }
                catch { /* fall through; Invoke surfaces the mismatch */ }
            }
            return v;
        }

        /// <summary>
        /// True when the interop proxy property is backed by a
        /// native FIELD (the generator emits a static
        /// NativeFieldInfoPtr_&lt;name&gt; alongside it). Reading such
        /// a property is a memory read; anything else is a native
        /// method call.
        /// </summary>
        private static bool IsFieldBackedProperty(PropertyInfo p)
        {
            return p.DeclaringType?.GetField(
                "NativeFieldInfoPtr_" + p.Name,
                BindingFlags.NonPublic | BindingFlags.Public | BindingFlags.Static) != null;
        }

        /// <summary>
        /// Rewrap a UnityEngine.Object wrapper as the requested
        /// interop proxy type so reflection sees the game class.
        /// Every generated proxy has a public (IntPtr) ctor.
        /// Falls back to the wrapper if the cast fails.
        /// </summary>
        private static object DowncastToProxy(Type t, UnityEngine.Object o)
        {
            try
            {
                return Activator.CreateInstance(t, o.Pointer) ?? (object)o;
            }
            catch
            {
                return o;
            }
        }

        private static object SerializeSafe(object v)
        {
            if (v == null) return null;
            var t = v.GetType();
            if (t.IsPrimitive || v is string) return v;
            // Il2Cpp objects: Newtonsoft would dump wrapper
            // plumbing (Pointer, WasCollected, ...). Report the
            // proxy type + ToString + a live HANDLE instead, so
            // any complex value chains into the existing ops
            // (inspect_object / read_field / invoke_method on the
            // handle). This is what makes the control plane fully
            // generic for research: arrays chain via get_Item /
            // Length, lists via get_Item / Count, dictionaries via
            // get_Item(key). release_handle frees them.
            if (v is Il2CppInterop.Runtime.InteropTypes.Il2CppObjectBase il2cpp)
            {
                string s;
                try { s = v.ToString(); } catch { s = "<tostring failed>"; }
                return new JObject
                {
                    ["il2cpp_type"] = t.FullName,
                    ["ptr"] = (long)il2cpp.Pointer,
                    ["str"] = s,
                    ["handle"] = Acquire(v),
                };
            }
            try
            {
                // Try a round-trip through Newtonsoft; if it works,
                // pass through as-is for full structure.
                JsonConvert.SerializeObject(v);
                return v;
            }
            catch
            {
                return v.ToString();
            }
        }

        private static object ResolveSingleton(Type t)
        {
            // Singleton<T>.Instance pattern: walk the type's base
            // chain looking for a class named `Singleton<...>` with
            // a public static `Instance` property/field.
            var cur = t;
            while (cur != null && cur != typeof(object))
            {
                if (cur.IsGenericType && cur.GetGenericTypeDefinition().Name.StartsWith("Singleton"))
                {
                    var prop = cur.GetProperty("Instance", BindingFlags.Public | BindingFlags.Static);
                    if (prop != null) return prop.GetValue(null, null);
                    var field = cur.GetField("Instance", BindingFlags.Public | BindingFlags.Static);
                    if (field != null) return field.GetValue(null);
                }
                cur = cur.BaseType;
            }
            var p2 = t.GetProperty("Instance", BindingFlags.Public | BindingFlags.Static | BindingFlags.FlattenHierarchy);
            if (p2 != null) return p2.GetValue(null, null);
            var f2 = t.GetField("Instance", BindingFlags.Public | BindingFlags.Static | BindingFlags.FlattenHierarchy);
            if (f2 != null) return f2.GetValue(null);
            return null;
        }
    }
}
