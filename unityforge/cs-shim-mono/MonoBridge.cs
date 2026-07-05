// MonoBridge.cs. Exposes Mono reflection helpers to Rust as
// function pointers.
//
// Handle model: every managed reference Rust holds is a 32-bit
// cookie into _handles. The dictionary is the single source of
// truth; Rust never sees a raw pointer.
//
// Newtonsoft.Json is the serialization format on the JSON
// out-buffers. Every game we target ships it.

using System;
using System.Collections.Generic;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

namespace Unityforge.Shim
{
    /// <summary>
    /// IBackendBridge adapter so Bridge.Build can populate the
    /// neutral-named struct from MonoBridge's Mono-flavored
    /// delegates.
    /// </summary>
    public sealed class MonoBackendBridge : IBackendBridge
    {
        public RuntimeKind Kind => RuntimeKind.Mono;
        public IntPtr FindType => Marshal.GetFunctionPointerForDelegate(MonoBridge.FindTypeDelegate);
        public IntPtr SingletonInstance => Marshal.GetFunctionPointerForDelegate(MonoBridge.SingletonInstanceDelegate);
        public IntPtr StaticInstance => Marshal.GetFunctionPointerForDelegate(MonoBridge.StaticInstanceDelegate);
        public IntPtr WalkClass => Marshal.GetFunctionPointerForDelegate(MonoBridge.WalkClassDelegate);
        public IntPtr InspectObject => Marshal.GetFunctionPointerForDelegate(MonoBridge.InspectObjectDelegate);
        public IntPtr ReadField => Marshal.GetFunctionPointerForDelegate(MonoBridge.ReadFieldDelegate);
        public IntPtr WriteField => Marshal.GetFunctionPointerForDelegate(MonoBridge.WriteFieldDelegate);
        public IntPtr InvokeMethod => Marshal.GetFunctionPointerForDelegate(MonoBridge.InvokeMethodDelegate);
        public IntPtr ReleaseHandle => Marshal.GetFunctionPointerForDelegate(MonoBridge.ReleaseHandleDelegate);
        public IntPtr ListMethods => Marshal.GetFunctionPointerForDelegate(MonoBridge.ListMethodsDelegate);
        public IntPtr InvokeStatic => Marshal.GetFunctionPointerForDelegate(MonoBridge.InvokeStaticDelegate);
    }

    public static class MonoBridge
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
        /// Drop every cached handle. Used during hot reload so
        /// the new Rust image starts with a fresh handle space.
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
        public delegate int ListMethodsFn(IntPtr typeNameUtf8, IntPtr outBuf, int cap);

        // Static delegate refs keep the closures alive against the GC.
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
        public static readonly ReleaseHandleFn ReleaseHandleDelegate = ReleaseHandle;
        public static readonly ListMethodsFn ListMethodsDelegate = ListMethods;

        // ---- implementations -------------------------------------------

        private static int FindType(IntPtr nameUtf8)
        {
            var name = Marshal.PtrToStringAnsi(nameUtf8);
            if (string.IsNullOrEmpty(name)) return 0;
            var t = TypeCache.Resolve(name);
            return Acquire(t);
        }

        private static int SingletonInstance(int typeHandle)
        {
            if (!(Lookup(typeHandle) is Type t)) return 0;
            try
            {
                var singleton = TypeCache.GenericSingletonInstance(t);
                return Acquire(singleton);
            }
            catch { return 0; }
        }

        private static int StaticInstance(int typeHandle)
        {
            if (!(Lookup(typeHandle) is Type t)) return 0;
            try
            {
                var inst = TypeCache.GenericStaticInstance(t);
                return Acquire(inst);
            }
            catch { return 0; }
        }

        private static int WalkClass(int typeHandle, int includeInactive, IntPtr outBuf, int cap)
        {
            if (!(Lookup(typeHandle) is Type t)) return WriteJson(outBuf, cap, new JObject { ["instances"] = new JArray() });
            var inst = new JArray();
            // For MonoBehaviours: UnityEngine.Object.FindObjectsOfType. For
            // arbitrary classes we leave the array empty (caller can use
            // singleton: / static_instance: selectors).
            try
            {
                if (typeof(UnityEngine.Object).IsAssignableFrom(t))
                {
                    var all = UnityEngine.Object.FindObjectsOfType(t, includeInactive != 0);
                    foreach (var o in all)
                    {
                        if (o == null) continue;
                        var name = (o as UnityEngine.Object)?.name ?? t.Name;
                        var h = Acquire(o);
                        inst.Add(new JObject { ["handle"] = h, ["name"] = name });
                    }
                }
            }
            catch { /* swallow; return empty */ }
            return WriteJson(outBuf, cap, new JObject { ["class"] = t.FullName, ["instances"] = inst });
        }

        private static int InspectObject(int handle, IntPtr outBuf, int cap)
        {
            var obj = Lookup(handle);
            if (obj == null) return WriteJson(outBuf, cap, new JObject { ["error"] = "null handle" });
            var t = obj.GetType();
            var props = new JArray();
            foreach (var f in t.GetFields(BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance))
            {
                JToken v;
                try { v = JsonValue(f.GetValue(obj)); }
                catch (Exception e) { v = "<error: " + e.Message + ">"; }
                props.Add(new JObject { ["name"] = f.Name, ["kind"] = "field", ["type"] = f.FieldType.Name, ["value"] = v });
            }
            foreach (var p in t.GetProperties(BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance))
            {
                if (p.GetIndexParameters().Length > 0) continue;
                JToken v;
                try { v = JsonValue(p.GetValue(obj, null)); }
                catch (Exception e) { v = "<error: " + e.Message + ">"; }
                props.Add(new JObject { ["name"] = p.Name, ["kind"] = "property", ["type"] = p.PropertyType.Name, ["value"] = v });
            }
            return WriteJson(outBuf, cap, new JObject { ["class_name"] = t.FullName, ["properties"] = props });
        }

        private static int ReadField(int handle, IntPtr fieldNameUtf8, IntPtr outBuf, int cap)
        {
            var obj = Lookup(handle);
            if (obj == null) return -1;
            var field = Marshal.PtrToStringAnsi(fieldNameUtf8);
            if (string.IsNullOrEmpty(field)) return -1;
            var t = obj.GetType();
            var fi = t.GetField(field, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
            if (fi != null)
            {
                return WriteJson(outBuf, cap, JsonValue(fi.GetValue(obj)));
            }
            var pi = t.GetProperty(field, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
            if (pi != null)
            {
                return WriteJson(outBuf, cap, JsonValue(pi.GetValue(obj, null)));
            }
            return -1;
        }

        private static int WriteField(int handle, IntPtr fieldNameUtf8, IntPtr valueJsonUtf8)
        {
            var obj = Lookup(handle);
            if (obj == null) return -2;
            var field = Marshal.PtrToStringAnsi(fieldNameUtf8);
            var valueJson = Marshal.PtrToStringAnsi(valueJsonUtf8);
            if (string.IsNullOrEmpty(field)) return -2;
            var t = obj.GetType();
            var fi = t.GetField(field, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
            if (fi != null)
            {
                try
                {
                    var converted = ConvertFromJson(valueJson, fi.FieldType);
                    fi.SetValue(obj, converted);
                    return 0;
                }
                catch { return -1; }
            }
            var pi = t.GetProperty(field, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
            if (pi != null && pi.CanWrite)
            {
                try
                {
                    var converted = ConvertFromJson(valueJson, pi.PropertyType);
                    pi.SetValue(obj, converted, null);
                    return 0;
                }
                catch { return -1; }
            }
            return -2;
        }

        private static int InvokeMethod(int handle, IntPtr methodNameUtf8, IntPtr argsJsonUtf8, IntPtr outBuf, int cap)
        {
            var obj = Lookup(handle);
            if (obj == null) return -1;
            var method = Marshal.PtrToStringAnsi(methodNameUtf8);
            var argsJson = Marshal.PtrToStringAnsi(argsJsonUtf8) ?? "[]";
            if (string.IsNullOrEmpty(method)) return -1;
            var t = obj.GetType();
            var argTokens = JArray.Parse(argsJson);
            var candidates = t.GetMethods(BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
            foreach (var mi in candidates)
            {
                if (mi.Name != method) continue;
                var pars = mi.GetParameters();
                if (pars.Length != argTokens.Count) continue;
                object[] callArgs;
                try
                {
                    callArgs = new object[pars.Length];
                    for (int i = 0; i < pars.Length; i++)
                        callArgs[i] = ConvertFromJsonToken(argTokens[i], pars[i].ParameterType);
                }
                catch { continue; }
                try
                {
                    var ret = mi.Invoke(obj, callArgs);
                    return WriteJson(outBuf, cap, JsonValue(ret));
                }
                catch (Exception e)
                {
                    return WriteJsonReturn(outBuf, cap, new JObject { ["error"] = e.Message }, -3);
                }
            }
            return -2;
        }

        // v6: invoke a STATIC method on a named class. Mirrors
        // InvokeMethod (same arg conversion, same result marshal,
        // same return codes) with a type-name resolve and a null
        // invocation target.
        private static int InvokeStatic(IntPtr classNameUtf8, IntPtr methodNameUtf8, IntPtr argsJsonUtf8, IntPtr outBuf, int cap)
        {
            var className = Marshal.PtrToStringAnsi(classNameUtf8);
            var method = Marshal.PtrToStringAnsi(methodNameUtf8);
            var argsJson = Marshal.PtrToStringAnsi(argsJsonUtf8) ?? "[]";
            if (string.IsNullOrEmpty(className) || string.IsNullOrEmpty(method)) return -1;
            var t = TypeCache.Resolve(className);
            if (t == null) return -1;
            var argTokens = JArray.Parse(argsJson);
            var candidates = t.GetMethods(BindingFlags.Public | BindingFlags.NonPublic
                | BindingFlags.Static | BindingFlags.FlattenHierarchy);
            foreach (var mi in candidates)
            {
                if (mi.Name != method) continue;
                var pars = mi.GetParameters();
                if (pars.Length != argTokens.Count) continue;
                object[] callArgs;
                try
                {
                    callArgs = new object[pars.Length];
                    for (int i = 0; i < pars.Length; i++)
                        callArgs[i] = ConvertFromJsonToken(argTokens[i], pars[i].ParameterType);
                }
                catch { continue; }
                try
                {
                    var ret = mi.Invoke(null, callArgs);
                    return WriteJson(outBuf, cap, JsonValue(ret));
                }
                catch (Exception e)
                {
                    return WriteJsonReturn(outBuf, cap, new JObject { ["error"] = e.Message }, -3);
                }
            }
            return -2;
        }

        private static int ListMethods(IntPtr typeNameUtf8, IntPtr outBuf, int cap)
        {
            var name = Marshal.PtrToStringAnsi(typeNameUtf8);
            if (string.IsNullOrEmpty(name))
                return WriteJson(outBuf, cap, new JObject { ["error"] = "type name required" });
            var t = TypeCache.Resolve(name);
            if (t == null)
                return WriteJson(outBuf, cap, new JObject { ["error"] = $"type '{name}' not found" });

            // Walk the inheritance chain so inherited methods on the
            // class (Singleton<T>, MonoBehaviour, etc.) are reported
            // alongside declared methods. Each entry tags the
            // declaring type so Harmony patch decisions are
            // unambiguous.
            var methods = new JArray();
            var seen = new HashSet<string>();
            var cur = t;
            while (cur != null && cur != typeof(object))
            {
                var mis = cur.GetMethods(BindingFlags.Public | BindingFlags.NonPublic
                    | BindingFlags.Instance | BindingFlags.Static | BindingFlags.DeclaredOnly);
                foreach (var mi in mis)
                {
                    var pars = mi.GetParameters();
                    var sb = new System.Text.StringBuilder();
                    sb.Append(mi.Name).Append("(");
                    for (int i = 0; i < pars.Length; i++)
                    {
                        if (i > 0) sb.Append(",");
                        sb.Append(pars[i].ParameterType.Name);
                    }
                    sb.Append(")");
                    var sig = cur.FullName + "::" + sb.ToString();
                    if (!seen.Add(sig)) continue;
                    methods.Add(new JObject
                    {
                        ["name"] = mi.Name,
                        ["declared_on"] = cur.FullName,
                        ["params"] = pars.Length,
                        ["static"] = mi.IsStatic,
                        ["return"] = mi.ReturnType?.Name ?? "void",
                    });
                }
                cur = cur.BaseType;
            }
            return WriteJson(outBuf, cap, new JObject
            {
                ["type"] = t.FullName,
                ["methods"] = methods,
            });
        }

        private static void ReleaseHandle(int handle)
        {
            if (handle == 0) return;
            lock (_lock) { _handles.Remove(handle); }
        }

        // ---- helpers ---------------------------------------------------

        private static JToken JsonValue(object v)
        {
            if (v == null) return JValue.CreateNull();
            switch (v)
            {
                case bool b: return new JValue(b);
                case sbyte i: return new JValue((long)i);
                case byte i: return new JValue((long)i);
                case short i: return new JValue((long)i);
                case ushort i: return new JValue((long)i);
                case int i: return new JValue((long)i);
                case uint i: return new JValue((long)i);
                case long i: return new JValue(i);
                case ulong i: return new JValue((long)i);
                case float f: return new JValue(f);
                case double f: return new JValue(f);
                case string s: return new JValue(s);
                case UnityEngine.Object uo: return new JObject { ["handle"] = Acquire(uo), ["name"] = uo == null ? "<null>" : uo.name, ["type"] = uo == null ? "?" : uo.GetType().Name };
                default:
                    var t = v.GetType();
                    if (t.IsEnum) return new JValue(v.ToString());
                    if (t.IsClass) return new JObject { ["handle"] = Acquire(v), ["type"] = t.Name };
                    return StructToJson(v, 0);
            }
        }

        /// <summary>
        /// Generic value-type serialization: a JSON object of the
        /// struct's public instance fields (recursive, depth-
        /// capped). Covers game structs like TerrainCoord /
        /// TerrainRect so Rust can read them as data instead of
        /// parsing ToString.
        /// </summary>
        private static JToken StructToJson(object v, int depth)
        {
            var t = v.GetType();
            if (depth >= 3) return new JValue(v.ToString());
            var fields = t.GetFields(BindingFlags.Public | BindingFlags.Instance);
            if (fields.Length == 0) return new JValue(v.ToString());
            var o = new JObject();
            foreach (var f in fields)
            {
                var fv = f.GetValue(v);
                if (fv == null) { o[f.Name] = JValue.CreateNull(); continue; }
                var ft = fv.GetType();
                if (ft.IsPrimitive || fv is string || ft.IsEnum)
                    o[f.Name] = JsonValue(fv);
                else if (ft.IsValueType)
                    o[f.Name] = StructToJson(fv, depth + 1);
                else
                    o[f.Name] = JsonValue(fv);
            }
            return o;
        }

        private static object ConvertFromJson(string json, Type t)
        {
            return ConvertFromJsonToken(JToken.Parse(json), t);
        }

        private static object ConvertFromJsonToken(JToken tok, Type t)
        {
            // JSON null -> null argument (e.g. the trailing object
            // params of GameTerrain.IsImpassable).
            if (tok == null || tok.Type == JTokenType.Null) return null;
            if (t == typeof(bool)) return tok.Value<bool>();
            if (t == typeof(int)) return tok.Value<int>();
            if (t == typeof(uint)) return (uint)tok.Value<long>();
            if (t == typeof(long)) return tok.Value<long>();
            if (t == typeof(float)) return tok.Value<float>();
            if (t == typeof(double)) return tok.Value<double>();
            if (t == typeof(string)) return tok.Value<string>();
            if (t.IsEnum) return Enum.Parse(t, tok.Value<string>());
            // Unity struct types from { x, y, z [, w] }.
            if (t == typeof(UnityEngine.Vector2) && tok is JObject v2o)
                return new UnityEngine.Vector2(
                    v2o["x"]?.Value<float>() ?? 0f,
                    v2o["y"]?.Value<float>() ?? 0f);
            if (t == typeof(UnityEngine.Vector3) && tok is JObject v3o)
                return new UnityEngine.Vector3(
                    v3o["x"]?.Value<float>() ?? 0f,
                    v3o["y"]?.Value<float>() ?? 0f,
                    v3o["z"]?.Value<float>() ?? 0f);
            if (t == typeof(UnityEngine.Vector4) && tok is JObject v4o)
                return new UnityEngine.Vector4(
                    v4o["x"]?.Value<float>() ?? 0f,
                    v4o["y"]?.Value<float>() ?? 0f,
                    v4o["z"]?.Value<float>() ?? 0f,
                    v4o["w"]?.Value<float>() ?? 0f);
            if (t == typeof(UnityEngine.Quaternion) && tok is JObject qo)
                return new UnityEngine.Quaternion(
                    qo["x"]?.Value<float>() ?? 0f,
                    qo["y"]?.Value<float>() ?? 0f,
                    qo["z"]?.Value<float>() ?? 0f,
                    qo["w"]?.Value<float>() ?? 1f);
            // object handle: { "handle": <int> }
            if (tok is JObject jo && jo["handle"] != null)
            {
                var h = jo["handle"].Value<int>();
                return Lookup(h);
            }
            // Generic value-type path: construct the struct and set
            // its public fields from a JSON object (mirror of
            // StructToJson; covers game structs like TerrainCoord /
            // TerrainRect as invoke args and field writes).
            if (t.IsValueType && tok is JObject sobj)
            {
                var boxed = Activator.CreateInstance(t);
                foreach (var prop in sobj.Properties())
                {
                    var f = t.GetField(prop.Name,
                        BindingFlags.Public | BindingFlags.Instance | BindingFlags.IgnoreCase);
                    if (f != null)
                    {
                        f.SetValue(boxed, ConvertFromJsonToken(prop.Value, f.FieldType));
                    }
                }
                return boxed;
            }
            throw new InvalidOperationException("unsupported target type: " + t.FullName);
        }

        private static int WriteJson(IntPtr outBuf, int cap, JToken value)
        {
            return WriteJsonReturn(outBuf, cap, value, -999); // success returns bytes written
        }

        private static int WriteJsonReturn(IntPtr outBuf, int cap, JToken value, int errorCode)
        {
            var s = value.ToString(Formatting.None);
            var bytes = Encoding.UTF8.GetBytes(s);
            if (bytes.Length >= cap) return errorCode == -999 ? -1 : errorCode;
            Marshal.Copy(bytes, 0, outBuf, bytes.Length);
            // NUL terminator
            Marshal.WriteByte(outBuf, bytes.Length, 0);
            return errorCode == -999 ? bytes.Length : errorCode;
        }
    }

    /// <summary>
    /// Resolves type names by walking every loaded assembly.
    /// Caches by name -> Type. Also handles `Singleton<T>` /
    /// `StaticInstance<T>` instance access via reflection.
    /// </summary>
    internal static class TypeCache
    {
        private static readonly Dictionary<string, Type> _byName = new Dictionary<string, Type>();

        public static Type Resolve(string name)
        {
            if (_byName.TryGetValue(name, out var t)) return t;
            // Pass 1: exact (namespace-qualified) match across every
            // assembly. A global-namespace game type ("Character")
            // is an exact match here and can never be shadowed by a
            // namespaced type sharing the short name. The old
            // single-pass version short-name-scanned each assembly
            // before trying the next one, so
            // UnityEngine.TextCore.Character won over the game's
            // Character and silently killed the AddInjury patch
            // (survivalist, 2026-07-04).
            foreach (var asm in AppDomain.CurrentDomain.GetAssemblies())
            {
                t = asm.GetType(name);
                if (t != null) break;
            }
            // Pass 2: short-name scan for callers naming a
            // namespaced type by its short name.
            if (t == null)
            {
                foreach (var asm in AppDomain.CurrentDomain.GetAssemblies())
                {
                    foreach (var x in asm.GetTypes())
                    {
                        if (x.Name == name) { t = x; break; }
                    }
                    if (t != null) break;
                }
            }
            if (t != null) _byName[name] = t;
            return t;
        }

        /// <summary>
        /// Try to read the `Instance` static property on a
        /// `Singleton{T}` parent. Many games (Wild West Miner etc)
        /// shape their managers as `class PlayerManager : Singleton{PlayerManager}`.
        /// </summary>
        public static object GenericSingletonInstance(Type t)
        {
            return ResolveInstanceProperty(t, "Singleton");
        }

        public static object GenericStaticInstance(Type t)
        {
            return ResolveInstanceProperty(t, "StaticInstance");
        }

        private static object ResolveInstanceProperty(Type t, string parentName)
        {
            var cur = t;
            while (cur != null && cur != typeof(object))
            {
                if (cur.IsGenericType && cur.GetGenericTypeDefinition().Name.StartsWith(parentName))
                {
                    var prop = cur.GetProperty("Instance", BindingFlags.Public | BindingFlags.Static);
                    if (prop != null) return prop.GetValue(null, null);
                    var field = cur.GetField("Instance", BindingFlags.Public | BindingFlags.Static);
                    if (field != null) return field.GetValue(null);
                }
                cur = cur.BaseType;
            }
            // Fall back: same-named static Instance on t itself
            var p2 = t.GetProperty("Instance", BindingFlags.Public | BindingFlags.Static | BindingFlags.FlattenHierarchy);
            if (p2 != null) return p2.GetValue(null, null);
            var f2 = t.GetField("Instance", BindingFlags.Public | BindingFlags.Static | BindingFlags.FlattenHierarchy);
            if (f2 != null) return f2.GetValue(null);
            return null;
        }
    }
}
