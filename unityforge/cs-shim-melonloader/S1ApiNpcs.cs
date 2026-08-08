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
        public static string SpawnGoon(float x, float y, float z)
            => Spawn(() => new GoonNpc(), x, y, z);

        public static string SpawnPolice(float x, float y, float z)
            => Spawn(() => new PoliceNpc(), x, y, z);

        public static string SpawnPlayerNpc(float x, float y, float z)
            => Spawn(() => new PlayerNpc(), x, y, z);

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
                return "{\"ok\":true,\"name\":\"" + npc.FirstName + " " + npc.LastName +
                    "\",\"queued\":true}";
            }
            catch (Exception e)
            {
                return Fail(e);
            }
        }

        private static string Fail(Exception e)
        {
            while (e is System.Reflection.TargetInvocationException tie && tie.InnerException != null)
                e = tie.InnerException;
            ShimLogger.Warn($"NpcFactory: {e.GetType().Name}: {e.Message}\n{e.StackTrace}");
            return "{\"ok\":false,\"error\":\"" +
                (e.GetType().Name + ": " + e.Message).Replace("\\", "\\\\").Replace("\"", "\\\"") + "\"}";
        }
    }
}
