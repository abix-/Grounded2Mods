using MGSC;

namespace QuasimorphMod
{
    public static class ModMain
    {
        [Hook(ModHookType.AfterConfigsLoaded)]
        public static void AfterConfigsLoaded(IModContext context)
        {
        }

        [Hook(ModHookType.DungeonStarted)]
        public static void DungeonStarted(IModContext context)
        {
        }
    }
}
