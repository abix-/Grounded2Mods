//! UE-side Tracker type alias.
//!
//! The full implementation lives in [`modforge::rpg::tracker`]
//! as the engine-generic `Tracker<E>`. UE call sites use the
//! alias below so they don't have to name the engine type
//! parameter.

pub use modforge::rpg::XpResult;

pub type Tracker = modforge::rpg::Tracker<crate::rpg::UeEngine>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpg::{
        Curve, Effect, EffectDef, FallEvent, KillEvent, SkillDef, SkillRegistry, TriggerCtx,
        trigger,
    };
    use std::sync::atomic::{AtomicU32, Ordering};

    struct CountingEffect {
        fires: AtomicU32,
        last_level: AtomicU32,
    }

    impl Effect<crate::rpg::UeEngine> for CountingEffect {
        fn apply(&self, level: u32, _max_level: u32, _ctx: &TriggerCtx<'_>) {
            self.fires.fetch_add(1, Ordering::Relaxed);
            self.last_level.store(level, Ordering::Relaxed);
        }
        fn format(&self, _l: u32, _m: u32) -> String {
            String::new()
        }
    }

    // Statics for the fake catalog (effects need 'static).
    static FX_KILL: CountingEffect = CountingEffect {
        fires: AtomicU32::new(0),
        last_level: AtomicU32::new(0),
    };
    static FX_FALL: CountingEffect = CountingEffect {
        fires: AtomicU32::new(0),
        last_level: AtomicU32::new(0),
    };
    static FX_SLOT: CountingEffect = CountingEffect {
        fires: AtomicU32::new(0),
        last_level: AtomicU32::new(0),
    };

    static FIRE_CATALOG: SkillRegistry = SkillRegistry::new(&[
        SkillDef {
            id: "fire_kill",
            display_name: "kill",
            max_level: 10,
            effect: EffectDef::new("Counting", &FX_KILL),
            trigger: &trigger::ON_KILL,
        },
        SkillDef {
            id: "fire_fall",
            display_name: "fall",
            max_level: 10,
            effect: EffectDef::new("Counting", &FX_FALL),
            trigger: &trigger::ON_FALL,
        },
        SkillDef {
            id: "fire_slot",
            display_name: "slot",
            max_level: 10,
            effect: EffectDef::new("Counting", &FX_SLOT),
            trigger: &trigger::ON_SLOT_CHANGE,
        },
    ]);

    // Stable static byte buffer cast to `&UObject`. The fake
    // effects in this test never deref the reference; they only
    // increment counters. So the pointer just needs to be
    // non-null and well-aligned.
    #[repr(C, align(16))]
    struct FakeObj([u8; 256]);
    static FAKE_OBJ: FakeObj = FakeObj([0; 256]);
    fn fake_uobject() -> &'static crate::ue::UObject {
        // SAFETY: FAKE_OBJ is a `'static` 256-byte aligned buffer;
        // casting its address to `*const UObject` gives a non-null
        // well-aligned pointer. The test never reads UObject
        // fields through this reference.
        unsafe { &*(&FAKE_OBJ as *const FakeObj as *const crate::ue::UObject) }
    }

    fn reset_counters() {
        FX_KILL.fires.store(0, Ordering::Relaxed);
        FX_FALL.fires.store(0, Ordering::Relaxed);
        FX_SLOT.fires.store(0, Ordering::Relaxed);
        FX_KILL.last_level.store(0, Ordering::Relaxed);
        FX_FALL.last_level.store(0, Ordering::Relaxed);
        FX_SLOT.last_level.store(0, Ordering::Relaxed);
    }

    fn make_tracker() -> Tracker {
        Tracker::new(&FIRE_CATALOG, Curve::new(100.0, 1.8, 50), "fire_test_slots")
    }

    #[test]
    fn fire_dispatch_end_to_end() {
        reset_counters();
        let t = make_tracker();

        // Phase 1: fire before slot active -> no fires.
        let fev = FallEvent {
            player: fake_uobject(),
            cmc: None,
            velocity_z_before: -500.0,
        };
        t.fire(&TriggerCtx::Engine(crate::rpg::UeEvent::Fall(&fev)));
        assert_eq!(FX_FALL.fires.load(Ordering::Relaxed), 0);

        // activate_slot applies only OnSlotChange-kinded skills.
        t.activate_slot("fire_test_slot".to_string());
        let kill_baseline = FX_KILL.fires.load(Ordering::Relaxed);
        let fall_baseline = FX_FALL.fires.load(Ordering::Relaxed);
        let slot_baseline = FX_SLOT.fires.load(Ordering::Relaxed);
        assert_eq!(kill_baseline, 0);
        assert_eq!(fall_baseline, 0);
        assert!(slot_baseline >= 1);

        // Phase 2: fire(Fall) at level 0 -> no additional fires.
        t.fire(&TriggerCtx::Engine(crate::rpg::UeEvent::Fall(&fev)));
        assert_eq!(FX_FALL.fires.load(Ordering::Relaxed), fall_baseline);

        // Phase 3: set levels directly + fire OnKill.
        t._test_with_inner_mut(|state| {
            state.skill_levels.insert("fire_kill".into(), 3);
            state.skill_levels.insert("fire_fall".into(), 7);
            state.skill_levels.insert("fire_slot".into(), 5);
        });
        let kev = KillEvent {
            victim: fake_uobject(),
            victim_class_name: "TestCreature",
            attacker: None,
            attacker_is_player: true,
            damage: 42.0,
        };
        t.fire(&TriggerCtx::Engine(crate::rpg::UeEvent::Kill(&kev)));
        assert_eq!(FX_KILL.fires.load(Ordering::Relaxed), kill_baseline + 1);
        assert_eq!(FX_KILL.last_level.load(Ordering::Relaxed), 3);
        assert_eq!(FX_FALL.fires.load(Ordering::Relaxed), fall_baseline);
        assert_eq!(FX_SLOT.fires.load(Ordering::Relaxed), slot_baseline);

        // Phase 4: OnFall fires the fall skill at its level.
        t.fire(&TriggerCtx::Engine(crate::rpg::UeEvent::Fall(&fev)));
        assert_eq!(FX_FALL.fires.load(Ordering::Relaxed), fall_baseline + 1);
        assert_eq!(FX_FALL.last_level.load(Ordering::Relaxed), 7);

        // Phase 5: disabled toggle suppresses the kill fire.
        t.disabled_skills().set("fire_kill", true);
        t.fire(&TriggerCtx::Engine(crate::rpg::UeEvent::Kill(&kev)));
        assert_eq!(FX_KILL.fires.load(Ordering::Relaxed), kill_baseline + 1);
    }
}
