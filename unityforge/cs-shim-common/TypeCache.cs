// TypeCache.cs. Backend-agnostic type-name resolution shared by
// every shim (Mono, IL2CPP, Survivalist, MelonLoader). Moved out
// of MonoBridge.cs 2026-08-07: HarmonyBridge.cs (shared) calls
// TypeCache.Resolve, so every shim that links HarmonyBridge needs
// this class, and the implementation is pure System.Reflection
// with nothing Mono-specific. On IL2CPP hosts the AppDomain walk
// sees the Il2CppInterop-generated proxy assemblies, which is
// exactly what Harmony patches target there.

using System;
using System.Collections.Generic;
using System.Reflection;
using Newtonsoft.Json.Linq;

namespace Unityforge.Shim
{
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
            // GetTypes() can throw on assemblies with unloadable
            // types (the 0.4.6f12 interop Il2Cppmscorlib has one;
            // an unknown class name crashed the whole game
            // 2026-08-08): take the loadable subset and skip
            // anything worse.
            if (t == null)
            {
                foreach (var asm in AppDomain.CurrentDomain.GetAssemblies())
                {
                    Type[] types;
                    try { types = asm.GetTypes(); }
                    catch (ReflectionTypeLoadException e) { types = e.Types; }
                    catch { continue; }
                    foreach (var x in types)
                    {
                        if (x != null && x.Name == name) { t = x; break; }
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

        /// <summary>
        /// The shared list_methods body (moved from MonoBridge.cs
        /// 2026-08-07 so the IL2CPP backend gets the same single
        /// implementation). Walks the inheritance chain so
        /// inherited methods on the class (Singleton&lt;T&gt;,
        /// MonoBehaviour, etc.) are reported alongside declared
        /// methods. Each entry tags the declaring type so Harmony
        /// patch decisions are unambiguous. Backends wrap this in
        /// their own buffer-write helper.
        /// </summary>
        public static JObject ListMethods(string name)
        {
            if (string.IsNullOrEmpty(name))
                return new JObject { ["error"] = "type name required" };
            var t = Resolve(name);
            if (t == null)
                return new JObject { ["error"] = $"type '{name}' not found" };

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
            return new JObject
            {
                ["type"] = t.FullName,
                ["methods"] = methods,
            };
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
