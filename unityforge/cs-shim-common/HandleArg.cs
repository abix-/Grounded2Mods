// HandleArg.cs. Shared resolution of the {"$handle": N} argument
// form: an invoke_method / write_field arg that references a LIVE
// object from the bridge's handle table instead of JSON data.
//
// This is the input half of generic op chaining (the output half
// is complex values carrying a "handle" in their JSON). Both
// backends support it via this one helper; each passes its own
// handle-table lookup.

using System;
using Newtonsoft.Json.Linq;

namespace Unityforge.Shim
{
    public static class HandleArg
    {
        public const string Key = "$handle";

        /// <summary>
        /// True when tok is {"$handle": N}; value is then the live
        /// object from the backend's handle table (null if the
        /// handle is stale, which the caller surfaces as a normal
        /// conversion failure).
        /// </summary>
        public static bool TryResolve(JToken tok, Func<int, object> lookup, out object value)
        {
            value = null;
            if (tok is JObject o
                && o.TryGetValue(Key, out var h)
                && (h.Type == JTokenType.Integer))
            {
                value = lookup((int)h);
                return true;
            }
            return false;
        }
    }
}
