// S1ApiNpcs.cs. SCHEDULE 1 SPECIFIC (the one game this
// MelonLoader shim serves): custom NPC types + a factory the
// Rust side drives via invoke_static (no bridge ABI involved).
//
// Rides S1API (already in the operator's mod stack): S1API owns
// the fragile clone-normalize-register-network-spawn chain
// (docs: ifbars.github.io/S1API basic-npc-creation; recipe
// verified against its source 2026-08-08). S1API auto-scans
// loaded assemblies for NPC subclasses and pre-registers their
// prefabs when FishNet is ready; constructing an instance
// (new GoonNpc()) builds the whole NPC.
//
// The S1API reference resolves lazily (first NpcFactory call),
// so the shim still loads if S1API is absent; the factory then
// reports the failure instead of crashing.
//
// Kinds (operator's list 2026-08-08): goons, police, and player
// NPCs, to start.

using System;

namespace Unityforge.Shim.Schedule1
{
    /// <summary>Hired muscle; garrisons and raids.</summary>
    public sealed class GoonNpc : S1API.Entities.NPC
    {
        public override bool IsPhysical => true;

        protected override void ConfigurePrefab(S1API.Entities.NPCPrefabBuilder builder)
        {
            builder.WithIdentity("modforge_goon", "Hired", "Muscle");
        }

        protected override void OnCreated()
        {
            base.OnCreated();
            Appearance.Build();
        }
    }

    /// <summary>Law pressure for the war's police layer.</summary>
    public sealed class PoliceNpc : S1API.Entities.NPC
    {
        public override bool IsPhysical => true;

        protected override void ConfigurePrefab(S1API.Entities.NPCPrefabBuilder builder)
        {
            builder.WithIdentity("modforge_police", "Beat", "Cop");
        }

        protected override void OnCreated()
        {
            base.OnCreated();
            Appearance.Build();
        }
    }

    /// <summary>The player's own people (player NPCs).</summary>
    public sealed class PlayerNpc : S1API.Entities.NPC
    {
        public override bool IsPhysical => true;

        protected override void ConfigurePrefab(S1API.Entities.NPCPrefabBuilder builder)
        {
            builder.WithIdentity("modforge_player_npc", "Loyal", "Soldier");
        }

        protected override void OnCreated()
        {
            base.OnCreated();
            Appearance.Build();
        }
    }

    /// <summary>
    /// Rust-facing factory. Every method is a plain public
    /// static reachable through the control plane's
    /// invoke_static; returns a JSON string.
    /// </summary>
    public static class NpcFactory
    {
        /// <summary>Minted NPCs by index (the Rust side's handle).</summary>
        private static readonly System.Collections.Generic.List<S1API.Entities.NPC> Minted =
            new System.Collections.Generic.List<S1API.Entities.NPC>();

        public static string SpawnGoon(float x, float y, float z)
            => Spawn(() => new GoonNpc(), x, y, z);

        public static string SpawnPolice(float x, float y, float z)
            => Spawn(() => new PoliceNpc(), x, y, z);

        public static string SpawnPlayerNpc(float x, float y, float z)
            => Spawn(() => new PlayerNpc(), x, y, z);

        /// <summary>Order a minted NPC onto the player.</summary>
        public static string AttackPlayer(int index)
        {
            try
            {
                var npc = Minted[index];
                npc.CombatBehaviour.SetAndAttackTarget(S1API.Entities.Player.Local);
                return "{\"ok\":true}";
            }
            catch (Exception e)
            {
                return Fail(e);
            }
        }

        /// <summary>Order minted NPC vs minted NPC (the war).</summary>
        public static string AttackNpc(int attacker, int target)
        {
            try
            {
                var a = Minted[attacker];
                var t = Minted[target];
                a.CombatBehaviour.SetAndAttackTarget(t);
                return "{\"ok\":true}";
            }
            catch (Exception e)
            {
                return Fail(e);
            }
        }

        /// <summary>
        /// Tough roll: raise a minted NPC's max health and heal
        /// it to full (per-instance MaxHealth; unlike the
        /// player's crash-prone static).
        /// </summary>
        public static string SetToughness(int index, float maxHealth)
        {
            try
            {
                var npc = Minted[index];
                npc.MaxHealth = maxHealth;
                npc.Heal((int)maxHealth);
                return "{\"ok\":true,\"max_health\":" +
                    maxHealth.ToString(System.Globalization.CultureInfo.InvariantCulture) + "}";
            }
            catch (Exception e)
            {
                return Fail(e);
            }
        }

        public static string SetSpeedMultiplier(int index, float multiplier)
        {
            try
            {
                var npc = Minted[index];
                float before = npc.Movement.SpeedMultiplier;
                npc.Movement.SpeedMultiplier = multiplier;
                float after = npc.Movement.SpeedMultiplier;
                return "{\"ok\":true,\"before\":" +
                    before.ToString(System.Globalization.CultureInfo.InvariantCulture) +
                    ",\"after\":" +
                    after.ToString(System.Globalization.CultureInfo.InvariantCulture) + "}";
            }
            catch (Exception e)
            {
                return Fail(e);
            }
        }

        /// <summary>
        /// Arm a minted NPC with a weapon by Resources path
        /// (e.g. "Avatar/Equippables/Knife", ".../M1911").
        /// </summary>
        public static string Arm(int index, string weaponPath)
        {
            try
            {
                var npc = Minted[index];
                npc.CombatBehaviour.SetCurrentWeapon(weaponPath);
                return "{\"ok\":true,\"weapon\":\"" + weaponPath + "\"}";
            }
            catch (Exception e)
            {
                return Fail(e);
            }
        }

        public static string KillNpc(int index)
        {
            try
            {
                var npc = Minted[index];
                npc.Kill();
                return "{\"ok\":true}";
            }
            catch (Exception e)
            {
                return Fail(e);
            }
        }

        public static string DespawnNpc(int index)
        {
            try
            {
                var npc = Minted[index];
                UnityEngine.Object.Destroy(npc.gameObject);
                return "{\"ok\":true}";
            }
            catch (Exception e)
            {
                return Fail(e);
            }
        }

        public static string CustomNpcCount()
        {
            try
            {
                return "{\"count\":" + S1API.Entities.NPC.All.Count +
                    ",\"ready\":" + (S1API.Entities.NPC.CustomNpcsReady ? "true" : "false") + "}";
            }
            catch (Exception e)
            {
                return Fail(e);
            }
        }

        private static string Spawn(Func<S1API.Entities.NPC> make, float x, float y, float z)
        {
            try
            {
                var npc = make();
                npc.gameObject.transform.position = new UnityEngine.Vector3(x, y, z);
                // The step S1API performs for save-load NPCs and a
                // bare constructor does not: queue the FishNet
                // network spawn + finalize pipeline (activation,
                // avatar). Internal, so reached via reflection.
                var patches = typeof(S1API.Entities.NPC).Assembly
                    .GetType("S1API.Internal.Patches.NPCPatches");
                var register = patches?.GetMethod(
                    "RegisterCustomNpcForNetworking",
                    System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Static);
                if (register == null)
                    return "{\"ok\":false,\"error\":\"RegisterCustomNpcForNetworking not found in S1API\"}";
                register.Invoke(null, new object[] { npc });
                int index;
                lock (Minted)
                {
                    index = Minted.Count;
                    Minted.Add(npc);
                }
                // The game-side NPC component's il2cpp pointer:
                // the identity our kill hooks see (NPCHealth.npc).
                long ptr = 0;
                try
                {
                    var s1npcField = typeof(S1API.Entities.NPC).GetField(
                        "S1NPC",
                        System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance);
                    if (s1npcField?.GetValue(npc) is Il2CppInterop.Runtime.InteropTypes.Il2CppObjectBase b)
                        ptr = (long)b.Pointer;
                }
                catch (Exception e)
                {
                    ShimLogger.Warn($"NpcFactory: npc ptr read failed: {e.Message}");
                }
                // Unique per-mint ID. Two stores must agree:
                // 1. NPCPrefabIdentity.Id on the clone's
                //    GameObject (so S1API's NPCStart postfix
                //    propagates the unique ID when Start fires).
                // 2. The framework data store (what wrapper.ID
                //    reads via NPCDataAccess.GetId). Without
                //    this, any ReconcileAllCustomNpc pass during
                //    the 3-6s settle window sees the shared
                //    prefab ID and warns per duplicate per pass.
                try
                {
                    var asm = typeof(S1API.Entities.NPC).Assembly;
                    var baseId = npc.ID;
                    var unique = (string.IsNullOrEmpty(baseId) ? "modforge" : baseId) + "_" + index;

                    // Store 1: the component (for Start).
                    var identityType = asm.GetType("S1API.Internal.Entities.NPCPrefabIdentity");
                    var getComp = typeof(UnityEngine.GameObject)
                        .GetMethod("GetComponent", System.Type.EmptyTypes)
                        ?.MakeGenericMethod(identityType);
                    var identity = getComp?.Invoke(npc.gameObject, null);
                    var idProp = identityType?.GetProperty(
                        "Id",
                        System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance);
                    if (identity != null && idProp != null)
                        idProp.SetValue(identity, unique);

                    // Store 2: the framework data (for wrapper.ID
                    // right now). NPC.ID { protected set } calls
                    // NPCDataAccess.ApplyId(S1NPC, value).
                    var wrapperIdProp = typeof(S1API.Entities.NPC).GetProperty("ID");
                    wrapperIdProp?.SetValue(npc, unique);
                }
                catch (Exception e)
                {
                    ShimLogger.Warn($"NpcFactory: unique-id assignment failed: {e.Message}");
                }
                return "{\"ok\":true,\"index\":" + index + ",\"ptr\":" + ptr + ",\"name\":\"" +
                    npc.FirstName + " " + npc.LastName + "\",\"queued\":true}";
            }
            catch (Exception e)
            {
                return Fail(e);
            }
        }

        /// <summary>
        /// Read the full behaviour state of a minted NPC:
        /// IdleBehaviour exists/active/enabled/idlePoint,
        /// behaviourStack count, enabledBehaviours count,
        /// activeBehaviour type+priority. Pure diagnostic.
        /// </summary>
        public static string GetBehaviourState(int index)
        {
            try
            {
                var npc = Minted[index];
                var s1npc = GetS1NPC(npc);
                if (s1npc == null)
                    return "{\"ok\":false,\"error\":\"S1NPC not resolved\"}";

                var beh = s1npc.Behaviour;
                if (beh == null)
                    return "{\"ok\":false,\"error\":\"NPCBehaviour is null\"}";

                int stackCount = 0;
                if (beh.behaviourStack != null)
                    stackCount = beh.behaviourStack.Count;

                int enabledCount = 0;
                if (beh.enabledBehaviours != null)
                    enabledCount = beh.enabledBehaviours.Count;

                string activeType = "null";
                int activePri = -999;
                var active = beh.activeBehaviour;
                if (active != null)
                {
                    activeType = active.GetIl2CppType().FullName;
                    activePri = active.Priority;
                }

                // Find IdleBehaviour in children
                var idle = beh.GetComponentInChildren<
                    Il2CppScheduleOne.NPCs.Behaviour.IdleBehaviour>(true);
                bool idleExists = idle != null;
                bool idleActive = false;
                bool idleEnabled = false;
                bool idlePointSet = false;
                int idleIndex = -1;
                if (idle != null)
                {
                    idleActive = idle.Active;
                    idleEnabled = idle.Enabled;
                    idlePointSet = idle.IdlePoint != null;
                    idleIndex = idle.BehaviourIndex;
                }

                // Build enabled list
                var enabledList = new System.Text.StringBuilder("[");
                if (beh.enabledBehaviours != null)
                {
                    for (int i = 0; i < beh.enabledBehaviours.Count; i++)
                    {
                        if (i > 0) enabledList.Append(",");
                        var b = beh.enabledBehaviours[i];
                        if (b != null)
                            enabledList.Append("{\"type\":\"" +
                                b.GetIl2CppType().Name + "\",\"pri\":" +
                                b.Priority + "}");
                    }
                }
                enabledList.Append("]");

                // Build stack list (types + priorities)
                var stackList = new System.Text.StringBuilder("[");
                if (beh.behaviourStack != null)
                {
                    for (int i = 0; i < beh.behaviourStack.Count; i++)
                    {
                        if (i > 0) stackList.Append(",");
                        var b = beh.behaviourStack[i];
                        if (b != null)
                            stackList.Append("{\"type\":\"" +
                                b.GetIl2CppType().Name +
                                "\",\"pri\":" + b.Priority +
                                ",\"active\":" + (b.Active ? "true" : "false") +
                                ",\"enabled\":" + (b.Enabled ? "true" : "false") +
                                ",\"idx\":" + b.BehaviourIndex + "}");
                    }
                }
                stackList.Append("]");

                // NPC position
                var pos = npc.gameObject.transform.position;

                return "{\"ok\":true" +
                    ",\"stack_count\":" + stackCount +
                    ",\"enabled_count\":" + enabledCount +
                    ",\"active_type\":\"" + activeType + "\"" +
                    ",\"active_pri\":" + activePri +
                    ",\"idle_exists\":" + (idleExists ? "true" : "false") +
                    ",\"idle_active\":" + (idleActive ? "true" : "false") +
                    ",\"idle_enabled\":" + (idleEnabled ? "true" : "false") +
                    ",\"idle_point_set\":" + (idlePointSet ? "true" : "false") +
                    ",\"idle_index\":" + idleIndex +
                    ",\"enabled_list\":" + enabledList +
                    ",\"stack\":" + stackList +
                    ",\"pos_x\":" + pos.x.ToString(System.Globalization.CultureInfo.InvariantCulture) +
                    ",\"pos_y\":" + pos.y.ToString(System.Globalization.CultureInfo.InvariantCulture) +
                    ",\"pos_z\":" + pos.z.ToString(System.Globalization.CultureInfo.InvariantCulture) +
                    "}";
            }
            catch (Exception e)
            {
                return Fail(e);
            }
        }

        /// <summary>
        /// Enable IdleBehaviour on a minted NPC using the same
        /// pattern S1API uses (LocationBasedActionSpec): activate
        /// the GameObject, Enable_Networked, ActivateBehaviour_Server.
        /// </summary>
        public static string EnableIdleBehaviour(int index)
        {
            try
            {
                var npc = Minted[index];
                var s1npc = GetS1NPC(npc);
                if (s1npc == null)
                    return "{\"ok\":false,\"error\":\"S1NPC not resolved\"}";
                var beh = s1npc.Behaviour;
                if (beh == null)
                    return "{\"ok\":false,\"error\":\"NPCBehaviour is null\"}";

                var idle = beh.GetComponentInChildren<
                    Il2CppScheduleOne.NPCs.Behaviour.IdleBehaviour>(true);
                if (idle == null)
                    return "{\"ok\":false,\"error\":\"IdleBehaviour not found\"}";

                if (idle.gameObject != null && !idle.gameObject.activeSelf)
                    idle.gameObject.SetActive(true);

                idle.Enable_Networked();
                if (idle.BehaviourIndex >= 0)
                    beh.ActivateBehaviour_Server(idle.BehaviourIndex);

                return GetBehaviourState(index);
            }
            catch (Exception e)
            {
                return Fail(e);
            }
        }

        /// <summary>
        /// Set IdleBehaviour's IdlePoint on a minted NPC to a
        /// world position, then enable the behaviour. Creates a
        /// persistent Transform at the target point.
        /// </summary>
        public static string SetIdlePoint(int index, float x, float y, float z)
        {
            try
            {
                var npc = Minted[index];
                var s1npc = GetS1NPC(npc);
                if (s1npc == null)
                    return "{\"ok\":false,\"error\":\"S1NPC not resolved\"}";
                var beh = s1npc.Behaviour;
                if (beh == null)
                    return "{\"ok\":false,\"error\":\"NPCBehaviour is null\"}";

                var idle = beh.GetComponentInChildren<
                    Il2CppScheduleOne.NPCs.Behaviour.IdleBehaviour>(true);
                if (idle == null)
                    return "{\"ok\":false,\"error\":\"IdleBehaviour not found\"}";

                // Create a persistent GameObject to hold the idle
                // point Transform (it must outlive the call).
                var pointGo = new UnityEngine.GameObject("IdlePoint_" + index);
                pointGo.transform.position = new UnityEngine.Vector3(x, y, z);
                // Parent it to the NPC so it doesn't get GC'd
                pointGo.transform.SetParent(npc.gameObject.transform, true);

                idle.IdlePoint = pointGo.transform;

                // Enable the behaviour
                if (idle.gameObject != null && !idle.gameObject.activeSelf)
                    idle.gameObject.SetActive(true);
                idle.Enable_Networked();
                if (idle.BehaviourIndex >= 0)
                    beh.ActivateBehaviour_Server(idle.BehaviourIndex);

                return GetBehaviourState(index);
            }
            catch (Exception e)
            {
                return Fail(e);
            }
        }

        /// <summary>
        /// Add FootPatrolBehaviour to a minted NPC, create a
        /// PatrolGroup, assign a named FootPatrolRoute, add the
        /// NPC to the group, refresh the behaviour stack.
        /// </summary>
        public static string AddFootPatrol(int index, string routeName)
        {
            try
            {
                var npc = Minted[index];
                var s1npc = GetS1NPC(npc);
                if (s1npc == null)
                    return "{\"ok\":false,\"error\":\"S1NPC not resolved\"}";
                var beh = s1npc.Behaviour;
                if (beh == null)
                    return "{\"ok\":false,\"error\":\"NPCBehaviour is null\"}";

                // Find the named FootPatrolRoute in the scene
                var allRoutes = UnityEngine.Object.FindObjectsOfType<
                    Il2CppScheduleOne.NPCs.Behaviour.FootPatrolRoute>(true);
                Il2CppScheduleOne.NPCs.Behaviour.FootPatrolRoute route = null;
                foreach (var r in allRoutes)
                {
                    if (r != null && r.RouteName == routeName)
                    {
                        route = r;
                        break;
                    }
                }
                if (route == null)
                    return "{\"ok\":false,\"error\":\"FootPatrolRoute '\" + routeName + \"' not found\"}";

                // Create FootPatrolBehaviour on a child GameObject
                var fpGo = new UnityEngine.GameObject("FootPatrolBehaviour");
                fpGo.transform.SetParent(beh.transform, false);
                var fpb = fpGo.AddComponent<
                    Il2CppScheduleOne.NPCs.Behaviour.FootPatrolBehaviour>();
                fpb.Priority = 3;

                // Set ownership (same pattern as S1API RepairBehaviourOwnership)
                fpb.beh = beh;

                // Init events
                fpb.onEnable ??= new UnityEngine.Events.UnityEvent();
                fpb.onDisable ??= new UnityEngine.Events.UnityEvent();
                fpb.onBegin ??= new UnityEngine.Events.UnityEvent();
                fpb.onEnd ??= new UnityEngine.Events.UnityEvent();

                // PatrolGroup has no default constructor in Il2Cpp bindings.
                // Needs further research on how the game creates these.
                return "{\"ok\":false,\"error\":\"AddFootPatrol not yet implemented: PatrolGroup constructor unknown\"}";
            }
            catch (Exception e)
            {
                return Fail(e);
            }
        }

        /// <summary>
        /// Enable FootPatrolBehaviour on a minted NPC (must have
        /// been added via AddFootPatrol first).
        /// </summary>
        public static string EnableFootPatrol(int index)
        {
            try
            {
                var npc = Minted[index];
                var s1npc = GetS1NPC(npc);
                if (s1npc == null)
                    return "{\"ok\":false,\"error\":\"S1NPC not resolved\"}";
                var beh = s1npc.Behaviour;
                if (beh == null)
                    return "{\"ok\":false,\"error\":\"NPCBehaviour is null\"}";

                var fpb = beh.GetComponentInChildren<
                    Il2CppScheduleOne.NPCs.Behaviour.FootPatrolBehaviour>(true);
                if (fpb == null)
                    return "{\"ok\":false,\"error\":\"FootPatrolBehaviour not found (call AddFootPatrol first)\"}";

                if (fpb.gameObject != null && !fpb.gameObject.activeSelf)
                    fpb.gameObject.SetActive(true);
                fpb.Enable_Networked();
                if (fpb.BehaviourIndex >= 0)
                    beh.ActivateBehaviour_Server(fpb.BehaviourIndex);

                return GetBehaviourState(index);
            }
            catch (Exception e)
            {
                return Fail(e);
            }
        }

        /// <summary>
        /// Get the S1NPC (game-side NPC component) from an S1API
        /// wrapper via reflection.
        /// </summary>
        private static Il2CppScheduleOne.NPCs.NPC GetS1NPC(S1API.Entities.NPC npc)
        {
            var field = typeof(S1API.Entities.NPC).GetField(
                "S1NPC",
                System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance);
            return field?.GetValue(npc) as Il2CppScheduleOne.NPCs.NPC;
        }

        private static string Fail(Exception e)
        {
            while (e is System.Reflection.TargetInvocationException tie && tie.InnerException != null)
                e = tie.InnerException;
            ShimLogger.Warn($"NpcFactory: {e.GetType().Name}: {e.Message}\n{e.StackTrace}");
            var msg = (e.GetType().Name + ": " + e.Message)
                .Replace("\\", "\\\\")
                .Replace("\"", "\\\"")
                .Replace("\r", "")
                .Replace("\n", " ");
            return "{\"ok\":false,\"error\":\"" + msg + "\"}";
        }
    }
}
