// G2's binder for `ueforge::damage::DamageHook`. The trampoline +
// parm decoding + classification + before/after dispatch all live
// framework-side; this file owns:
//
// - The Maine-specific `DamageHookConfig` (component class,
//   damage UFunction, parm offsets, FDamageInfo layout).
// - The KillerKind classifier (Player / Buggy / Other. G2 has
//   tame buggies in addition to direct player kills).
// - Per-event game-side reactions that aren't catalog skills:
//   kill credit (award XP), debug PE-queue drain, damage-trace
//   passthrough.
// - Fan-out to subscribed catalog Effects via TRACKER.fire for
//   the three damage-flavored TriggerCtx variants. Lifesteal +
//   impact-resistance reversal now live as Effects subscribed
//   to ON_DAMAGE_DEALT / ON_DAMAGE_TAKEN; pre-mutation slots
//   (Critical, Evasion) remain in `before`.

use std::sync::OnceLock;

use ueforge::damage::{DamageBinder, DamageEvent, DamageHook, DamageHookConfig};
use ueforge::hook::ProcessEventHook;
use ueforge::rpg::TriggerCtx;
use ueforge::ue::actor::{class_chain_contains, controller_pawn};
use ueforge::ue::damage_info::DamageInfoLayout;
use ueforge::ue::UObject;

/// Maine `FDamageInfo` layout. `last_damage_info_offset` is
/// `UHealthComponent.LastDamageInfo` at `0x03B0`; the four field
/// offsets are absolute (component-relative, already including
/// the base).
pub(crate) const DAMAGE_INFO: DamageInfoLayout = DamageInfoLayout {
    last_damage_info_offset: 0x03B0,
    instigator_offset: 0x03B0 + 0x0020,
    damage_source_offset: 0x03B0 + 0x0028,
    damage_type_class_offset: 0x03B0 + 0x0040,
    damage_flags_offset: 0x03B0 + 0x0070,
};

const CONFIG: DamageHookConfig = DamageHookConfig {
    component_class: "HealthComponent",
    damage_fn: "MulticastHandleEffectsWithDamageFlags",
    // Maine multicast layout: HitLocation(0x18) Damage(0x18-23
    // wait: HitLocation FVector_NetQuantize is 0x18 bytes at
    // +0x00, then Damage(f32) at +0x18, DamageFlags(i32) at
    // +0x1C, TypeFlags(u32) at +0x20.
    damage_parm_offset: 0x18,
    damage_flags_parm_offset: 0x1C,
    type_flags_parm_offset: 0x20,
    killing_blow_mask: 2, // EDamageInfoFlags::KillingBlow
    damage_info: DAMAGE_INFO,
    player_outer_filter: "BP_SurvivalPlayerCharacter",
    player_controller_filter: "PlayerController",
};

static HOOK: DamageHook<G2DamageBinder> = DamageHook::new(CONFIG);

// SurvivalCreature filter is needed for kill credit (excludes
// player deaths and prop / building destruction events that also
// fire MulticastHandleEffectsWithDamageFlags). Cached at install.
static CREATURE_CLASS: OnceLock<usize> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KillerKind {
    Player,
    Buggy,
    Other,
}

/// Classifies a killing controller as the player, a tame Buggy, or another source.
/// Stays here because Buggy ownership and kill-credit policy are specific to Grounded 2;
/// Ueforge owns reusable Unreal class and pawn lookup.
fn classify(instigator: Option<&UObject>) -> KillerKind {
    let Some(controller) = instigator else {
        return KillerKind::Other;
    };
    if class_chain_contains(controller, "PlayerController") {
        return KillerKind::Player;
    }
    if class_chain_contains(controller, "Buggy") {
        return KillerKind::Buggy;
    }
    if let Some(pawn) = controller_pawn(controller)
        && class_chain_contains(pawn, "Buggy")
    {
        return KillerKind::Buggy;
    }
    KillerKind::Other
}

/// Builds a readable Grounded 2 controller label for kill diagnostics.
/// Stays here because it is presentation for this mod's kill-credit log;
/// Ueforge owns object-name access.
fn describe_instigator(instigator: Option<&UObject>) -> String {
    let Some(ctrl) = instigator else {
        return "<unresolved>".to_string();
    };
    let class_name = ctrl
        .class()
        .map(|c| c.as_object().name())
        .unwrap_or_default();
    format!("{} ({})", ctrl.name(), class_name)
}

struct G2DamageBinder;

impl DamageBinder for G2DamageBinder {
    /// Records player damage and fires Grounded 2 skill effects before the engine handles the hit.
    /// Stays here because this mod chooses its diagnostics and pre-damage skill reactions;
    /// Ueforge owns damage decoding and hook dispatch.
    fn before(&self, event: &DamageEvent) -> Option<f32> {
        ueforge::counters::bump(&crate::counters::KILL_HOOK_FIRES);

        // Player-component diagnostic record (damage_ring + log).
        if event.victim_is_player {
            ueforge::counters::bump(&crate::counters::KILL_HOOK_PLAYER_FIRES);
            push_damage_event(event);
        }

        // Fire ON_DAMAGE_DEALT for player-instigator hits. Catalog
        // effects subscribed to this trigger run pre-engine (so
        // future Crit / Evasion can mutate damage). Lifesteal also
        // subscribes here; it runs post-engine in practice because
        // it only reads `event.damage`. Calling once per
        // player-instigator event is the doctrine.
        if event.attacker_is_player {
            crate::rpg::tracker::TRACKER.fire(&TriggerCtx::Engine(ueforge::rpg::UeEvent::DamageDealt(event)));
        }

        // Live-damage pre-mutation slots (Critical multiplier,
        // Evasion zero-on-roll) belong here. Catalog rows pending.
        None
    }

    /// Fires post-damage skills and awards Grounded 2 XP for confirmed creature kills.
    /// Stays here because creature eligibility, Buggy credit, and XP rewards are game-specific;
    /// Ueforge owns damage and kill event delivery.
    fn after(&self, event: &DamageEvent) {
        // Fire ON_DAMAGE_TAKEN for player-target hits. Impact
        // Resistance subscribes here (env-damage reversal on
        // HC.CurrentDamage runs after the engine wrote it).
        if event.victim_is_player {
            crate::rpg::tracker::TRACKER.fire(&TriggerCtx::Engine(ueforge::rpg::UeEvent::DamageTaken(event)));
        }

        // Kill credit on killing-blow events targeting creatures.
        // Stays in the binder (not a catalog skill): G2-side XP
        // bookkeeping, not a per-level effect. The OnKill trigger
        // still fires after credit logic so future kill-driven
        // skills (e.g. heal-on-kill) can subscribe.
        if !event.is_killing_blow {
            return;
        }
        let Some(creature_addr) = CREATURE_CLASS.get().copied() else {
            return;
        };
        let Some(victim) = event.victim else {
            return;
        };
        let Some(actor_class) = victim.class() else {
            return;
        };
        let creature_uclass = unsafe {
            &*(creature_addr as *const ueforge::ue::UClass)
        };
        if !victim.is_a(creature_uclass) {
            if cfg!(debug_assertions) {
                ueforge::log!(
                    "rpg/kill: ignoring non-creature kill: {} ({})",
                    victim.name(),
                    actor_class.as_object().name()
                );
            }
            return;
        }

        let class_name = actor_class.as_object().name();
        let kind = classify(event.attacker.map(|a| a as &UObject));
        let killer_label = describe_instigator(event.attacker.map(|a| a as &UObject));

        use crate::rpg::tracker::{record_kill, KillSource};
        let (label, source) = match kind {
            KillerKind::Player => ("PLAYER", Some(KillSource::Player)),
            KillerKind::Buggy => ("BUGGY", Some(KillSource::Buggy)),
            KillerKind::Other => ("skip", None),
        };
        ueforge::log!(
            "rpg/kill: {} {} ({}) killed by {}",
            label,
            victim.name(),
            class_name,
            killer_label
        );
        if let Some(s) = source {
            record_kill(&class_name, s);
        }

        // Fan-out to OnKill subscribers. KillEvent pre-resolves
        // the victim class name so subscribers don't re-pay the
        // FName lookup.
        let kev = ueforge::rpg::KillEvent {
            victim,
            victim_class_name: &class_name,
            attacker: event.attacker,
            attacker_is_player: event.attacker_is_player,
            damage: event.damage,
        };
        crate::rpg::tracker::TRACKER.fire(&TriggerCtx::Engine(ueforge::rpg::UeEvent::Kill(&kev)));
    }
}

/// Saves one Grounded 2 damage event for the debug API and writes its trace log.
/// Stays here because the recorded function, health offset, and trace policy belong to this mod;
/// Ueforge owns the reusable damage event and ring buffer.
fn push_damage_event(event: &DamageEvent) {
    use ueforge::ue::field::read_f32;
    let cd_now = read_f32(event.victim_component, 0x032C);
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    crate::debug::record_damage_event(crate::debug::DamageEvent {
        at_secs: now_secs,
        function: "MulticastHandleEffectsWithDamageFlags".to_string(),
        damage: event.damage,
        damage_flags: event.damage_flags,
        type_flags: event.type_flags,
        current_damage_before: Some(cd_now),
        current_damage_after: None,
    });
    if event.damage > 0.0 {
        ueforge::counters::bump(&crate::counters::KILL_HOOK_LOG_LINES);
        ueforge::log!(
            "rpg/dmg-trace: damage={:.2} flags=0x{:08x} type_flags=0x{:08x}",
            event.damage,
            event.damage_flags,
            event.type_flags
        );
    }
}

/// Installs Grounded 2's health-component damage hook and creature-class filter.
/// Stays here because the component, function, layout, and creature class are Grounded 2 facts;
/// Ueforge owns the reusable damage hook.
pub fn install() -> Result<ProcessEventHook, &'static str> {
    let creature_class = ueforge::ue::ClassRef::new("SurvivalCreature")
        .get()
        .ok_or("SurvivalCreature class not loaded")?;
    let _ = CREATURE_CLASS.set(
        creature_class as *const ueforge::ue::UClass as usize,
    );
    HOOK.install(G2DamageBinder)
}
