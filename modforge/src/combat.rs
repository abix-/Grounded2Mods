//! Combat decisions: health and hit resolution. The consumer does
//! hit detection (raycasts, overlap checks); this module decides the
//! outcome. Player and NPCs go through the same path.

use crate::item::CombatStats;

/// Health for any combatant: player and NPCs share this.
#[derive(Clone, Debug)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }

    pub fn fraction(&self) -> f32 {
        if self.max <= 0.0 {
            return 0.0;
        }
        (self.current / self.max).clamp(0.0, 1.0)
    }
}

/// The result of one hit resolved.
#[derive(Clone, Debug)]
pub struct HitResult {
    pub damage_dealt: f32,
    pub killed: bool,
}

/// The ONE function that resolves a hit. The consumer detects that a
/// hit landed (raycast, melee overlap); this function computes and
/// applies the damage. Returns what happened.
pub fn resolve_hit(weapon: &CombatStats, target: &mut Health) -> HitResult {
    let damage_dealt = weapon.damage;
    target.current = (target.current - damage_dealt).max(0.0);
    HitResult {
        damage_dealt,
        killed: target.is_dead(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::CombatStats;

    fn pipe() -> CombatStats {
        CombatStats {
            damage: 20.0,
            attack_speed: 1.0,
            range: 1.5,
            ammo: None,
        }
    }

    fn pistol() -> CombatStats {
        CombatStats {
            damage: 40.0,
            attack_speed: 0.8,
            range: 50.0,
            ammo: Some("pistol ammo".to_string()),
        }
    }

    #[test]
    fn hit_reduces_health() {
        let mut hp = Health::new(100.0);
        let result = resolve_hit(&pipe(), &mut hp);
        assert_eq!(result.damage_dealt, 20.0);
        assert!(!result.killed);
        assert_eq!(hp.current, 80.0);
    }

    #[test]
    fn lethal_hit_kills() {
        let mut hp = Health::new(30.0);
        let result = resolve_hit(&pistol(), &mut hp);
        assert_eq!(result.damage_dealt, 40.0);
        assert!(result.killed);
        assert_eq!(hp.current, 0.0);
    }

    #[test]
    fn health_never_goes_negative() {
        let mut hp = Health::new(10.0);
        resolve_hit(&pistol(), &mut hp);
        assert_eq!(hp.current, 0.0);
    }

    #[test]
    fn health_fraction() {
        let mut hp = Health::new(100.0);
        assert_eq!(hp.fraction(), 1.0);
        resolve_hit(&pipe(), &mut hp);
        assert!((hp.fraction() - 0.8).abs() < 0.001);
    }

    #[test]
    fn is_dead_after_zero() {
        let mut hp = Health::new(20.0);
        assert!(!hp.is_dead());
        resolve_hit(&pipe(), &mut hp);
        assert!(hp.is_dead());
    }
}
