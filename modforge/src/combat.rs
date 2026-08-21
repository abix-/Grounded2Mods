//! Combat decisions, engine-agnostic (topside design.md "How a hit
//! works"). The consumer finds hits (ray and shape casts in its
//! physics world) and moves bodies; this module decides everything
//! else: what a hit does, when a shooter may fire, how pellets add
//! up. Player and NPCs share every path.
//!
//! Prior art, read from source: Quake 3's `G_Damage` (one function
//! for every hit: armor, knockback, health, die or pain), Doom 3's
//! damage defs (damage as data a weapon names), Half-Life's
//! multi-damage (pellets summed per target), Quake 3's `PM_Weapon`
//! (fire rate as a timer on the shooter), Rust's falloff by
//! distance (Facepunch devblog 123).

/// What kind of hurt a hit is. Resistances multiply per type.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum DamageType {
    Blunt,
    Slash,
    Pierce,
    Bullet,
    Fire,
    Blast,
}

/// How damage falls off with distance: full up to `full_range`,
/// then straight down to `far_fraction` at `far_range` and beyond.
/// Rust: "rifles have always had a less severe damage falloff over
/// distance than pistols".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Falloff {
    pub full_range: f32,
    pub far_range: f32,
    pub far_fraction: f32,
}

impl Falloff {
    pub const NONE: Falloff = Falloff {
        full_range: f32::INFINITY,
        far_range: f32::INFINITY,
        far_fraction: 1.0,
    };

    pub fn at(&self, distance: f32) -> f32 {
        if distance <= self.full_range {
            1.0
        } else if distance >= self.far_range {
            self.far_fraction
        } else {
            let t = (distance - self.full_range) / (self.far_range - self.full_range);
            1.0 + (self.far_fraction - 1.0) * t
        }
    }
}

/// One kind of hurt as data (Doom 3's damage def): what a weapon,
/// projectile, or hazard names when it lands. `name` is the id.
#[derive(Clone, Debug, PartialEq)]
pub struct DamageDef {
    pub name: String,
    pub amount: f32,
    pub kind: DamageType,
    /// Velocity added to the target along the hit direction, scaled
    /// by the target's mass (Quake 3: damage over a mass of 200,
    /// capped at 200).
    pub knockback: f32,
    /// Armor is skipped (Doom 3 `noArmor`).
    pub ignores_armor: bool,
    /// Damage to yourself is scaled by this (Doom 3
    /// `selfDamageScale`; Quake 3 halves it so rocket jumping works).
    pub self_scale: f32,
    pub falloff: Falloff,
}

/// The checked-in damage defs. Consumers register content at startup
/// and weapons look defs up by name.
#[derive(Default)]
pub struct DamageRegistry {
    defs: Vec<DamageDef>,
}

impl DamageRegistry {
    pub fn register(&mut self, def: DamageDef) -> Result<(), String> {
        if self.defs.iter().any(|d| d.name == def.name) {
            return Err(format!("damage def '{}' registered twice", def.name));
        }
        self.defs.push(def);
        Ok(())
    }

    pub fn def(&self, name: &str) -> Option<&DamageDef> {
        self.defs.iter().find(|d| d.name == name)
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

/// Health for any combatant: player, NPCs, and building pieces.
#[derive(Clone, Debug, PartialEq)]
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

/// What stands between a hit and a target's health. `armor` is the
/// pool Quake 3 style (absorbs a share until spent); `resistances`
/// multiply by damage type (Valheim). Worn gear per body area feeds
/// this once actors have parts.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Protection {
    pub armor: f32,
    pub resistances: Vec<(DamageType, f32)>,
}

/// Share of damage armor absorbs while it lasts (Quake 3
/// `ARMOR_PROTECTION`).
pub const ARMOR_PROTECTION: f32 = 0.66;
/// Quake 3's player mass for knockback.
pub const KNOCKBACK_MASS: f32 = 200.0;
pub const KNOCKBACK_CAP: f32 = 200.0;

impl Protection {
    pub fn resistance(&self, kind: DamageType) -> f32 {
        self.resistances
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, m)| *m)
            .unwrap_or(1.0)
    }
}

/// One hit about to be resolved: who, what, where.
#[derive(Clone, Debug)]
pub struct Hit<'a> {
    pub def: &'a DamageDef,
    /// True when the attacker is the target (rocket jump, own grenade).
    pub self_inflicted: bool,
    /// Distance from the shooter to the point of impact, for falloff.
    pub distance: f32,
    /// Multiplier for where it landed (body part), 1.0 for none.
    pub location_scale: f32,
}

/// Where on a body a hit landed, by height up the body (Rust: head
/// shots hurt double, legs less). Each area is guarded by the gear
/// worn in its equipment slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyArea {
    Head,
    Chest,
    Legs,
}

impl BodyArea {
    /// From the hit's height as a fraction of body height, feet 0 to
    /// top 1.
    pub fn from_height(fraction: f32) -> BodyArea {
        if fraction >= 0.85 {
            BodyArea::Head
        } else if fraction >= 0.45 {
            BodyArea::Chest
        } else {
            BodyArea::Legs
        }
    }

    /// Damage multiplier for the area (Rust's head 2x, legs 0.75x).
    pub fn scale(self) -> f32 {
        match self {
            BodyArea::Head => 2.0,
            BodyArea::Chest => 1.0,
            BodyArea::Legs => 0.75,
        }
    }

    /// The equipment slot whose worn item guards this area.
    pub fn slot(self) -> crate::item::EquipSlot {
        match self {
            BodyArea::Head => crate::item::EquipSlot::Head,
            BodyArea::Chest => crate::item::EquipSlot::Chest,
            BodyArea::Legs => crate::item::EquipSlot::Legs,
        }
    }
}

impl Protection {
    /// This actor's protection for a hit on `area`: the base plus the
    /// armor of whatever is worn in the area's slot. `armor_of` looks
    /// an item name up in the consumer's registry; gear worn in the
    /// wrong slot guards nothing.
    pub fn for_area(
        &self,
        area: BodyArea,
        worn: &crate::item::Equipment,
        armor_of: impl Fn(&str) -> Option<crate::item::Armor>,
    ) -> Protection {
        let worn_armor = worn
            .get(area.slot())
            .and_then(|stack| armor_of(&stack.item))
            .filter(|armor| armor.slot == area.slot())
            .map(|armor| armor.amount)
            .unwrap_or(0.0);
        Protection {
            armor: self.armor + worn_armor,
            resistances: self.resistances.clone(),
        }
    }
}

/// The directions of one shot's pellets: the aim, each pellet pushed
/// off it by up to `spread_degrees` in a deterministic pattern from
/// `seed` (the tick), so a test and a replay land the same shot. One
/// pellet with no spread is the aim itself.
pub fn pellet_directions(
    forward: glam::Vec3,
    spread_degrees: f32,
    pellets: u32,
    seed: u64,
) -> Vec<glam::Vec3> {
    let forward = forward.normalize();
    if pellets <= 1 || spread_degrees <= 0.0 {
        return vec![forward; pellets.max(1) as usize];
    }
    let up = if forward.y.abs() < 0.99 {
        glam::Vec3::Y
    } else {
        glam::Vec3::X
    };
    let right = forward.cross(up).normalize();
    let up = right.cross(forward);
    let spread = spread_degrees.to_radians();
    (0..pellets)
        .map(|i| {
            // Two cheap hashes per pellet for angle and radius.
            let h = seed
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(u64::from(i).wrapping_mul(0xBF58_476D_1CE4_E5B9));
            let a = (h >> 11) as f32 / (1u64 << 53) as f32;
            let r = ((h.rotate_left(29) >> 11) as f32 / (1u64 << 53) as f32).sqrt();
            let angle = a * std::f32::consts::TAU;
            let off = r * spread;
            (forward + (right * angle.cos() + up * angle.sin()) * off.tan()).normalize()
        })
        .collect()
}

/// What one resolved hit did.
#[derive(Clone, Debug, PartialEq)]
pub struct HitResult {
    /// Taken off health.
    pub damage_dealt: f32,
    /// Taken off armor.
    pub armor_absorbed: f32,
    /// Speed to add to the target along the hit direction.
    pub knockback: f32,
    pub killed: bool,
}

/// THE damage function (Quake 3's `G_Damage`). Every hit in the game
/// ends here: the consumer detected it, this decides it. Order, as
/// Quake 3 does it: knockback is figured from the raw damage even
/// if armor takes it all; self damage is scaled; damage never drops
/// below 1; armor absorbs its share; health takes the rest; the
/// caller reads `killed` to run die or pain.
pub fn resolve_hit(hit: &Hit<'_>, protection: &mut Protection, target: &mut Health) -> HitResult {
    let def = hit.def;
    let mut damage = def.amount * def.falloff.at(hit.distance) * hit.location_scale;
    damage *= protection.resistance(def.kind);

    let knockback = (def.knockback * damage.min(KNOCKBACK_CAP)) / KNOCKBACK_MASS;

    if hit.self_inflicted {
        damage *= def.self_scale;
    }
    let damage = damage.max(1.0);

    let armor_absorbed = if def.ignores_armor || protection.armor <= 0.0 {
        0.0
    } else {
        let save = (damage * ARMOR_PROTECTION).ceil().min(protection.armor);
        protection.armor -= save;
        save
    };
    let take = damage - armor_absorbed;
    target.current = (target.current - take).max(0.0);
    HitResult {
        damage_dealt: take,
        armor_absorbed,
        knockback,
        killed: target.is_dead(),
    }
}

/// Pellets summed per target before one resolve each (Half-Life's
/// multi-damage): a shotgun blast is one hit on each thing it
/// touched, not ten pains.
#[derive(Debug)]
pub struct MultiDamage<T: PartialEq> {
    hits: Vec<(T, f32, u32)>,
}

impl<T: PartialEq> Default for MultiDamage<T> {
    fn default() -> Self {
        Self { hits: Vec::new() }
    }
}

impl<T: PartialEq + Clone> MultiDamage<T> {
    /// One pellet landed on `target` at `distance`.
    pub fn add(&mut self, target: T, distance: f32) {
        match self.hits.iter_mut().find(|(t, _, _)| *t == target) {
            Some((_, d, n)) => {
                *d += distance;
                *n += 1;
            }
            None => self.hits.push((target, distance, 1)),
        }
    }

    /// Each target hit, with how many pellets and their mean distance.
    pub fn drain(&mut self) -> Vec<(T, u32, f32)> {
        self.hits
            .drain(..)
            .map(|(t, d, n)| (t, n, d / n as f32))
            .collect()
    }
}

/// Fire rate as a timer on the shooter (Quake 3's `PM_Weapon`): the
/// weapon is ready when `ready_in` reaches zero; firing adds the
/// weapon's delay; firing empty adds a penalty instead.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FireTimer {
    pub ready_in: f32,
}

/// Quake 3: an empty click costs half a second.
pub const EMPTY_DELAY: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// Not ready yet, nothing happens.
    Waiting,
    /// Fire now; the delay has been added.
    Fire,
    /// Ready but no ammo; the penalty has been added.
    Empty,
}

impl FireTimer {
    pub fn tick(&mut self, dt: f32) {
        self.ready_in = (self.ready_in - dt).max(0.0);
    }

    /// The trigger is held: decide whether this step fires.
    /// `has_ammo` is true for melee and loaded guns.
    pub fn pull(&mut self, delay: f32, has_ammo: bool) -> Trigger {
        if self.ready_in > 0.0 {
            return Trigger::Waiting;
        }
        if !has_ammo {
            self.ready_in = EMPTY_DELAY;
            return Trigger::Empty;
        }
        self.ready_in = delay;
        Trigger::Fire
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipe() -> DamageDef {
        DamageDef {
            name: "pipe swing".to_string(),
            amount: 20.0,
            kind: DamageType::Blunt,
            knockback: 1.0,
            ignores_armor: false,
            self_scale: 0.5,
            falloff: Falloff::NONE,
        }
    }

    #[test]
    fn body_areas_by_height_with_rust_scales() {
        assert_eq!(BodyArea::from_height(0.95), BodyArea::Head);
        assert_eq!(BodyArea::from_height(0.6), BodyArea::Chest);
        assert_eq!(BodyArea::from_height(0.2), BodyArea::Legs);
        assert_eq!(BodyArea::Head.scale(), 2.0);
        assert_eq!(BodyArea::Legs.scale(), 0.75);
        assert_eq!(BodyArea::Head.slot(), crate::item::EquipSlot::Head);
    }

    #[test]
    fn worn_gear_guards_only_its_own_area() {
        let mut worn = crate::item::Equipment::default();
        worn.set(
            crate::item::EquipSlot::Chest,
            Some(crate::item::ItemStack {
                item: "vest".to_string(),
                count: 1,
                quality: None,
            }),
        );
        let armor_of = |name: &str| {
            (name == "vest").then_some(crate::item::Armor {
                slot: crate::item::EquipSlot::Chest,
                amount: 30.0,
            })
        };
        let base = Protection::default();
        assert_eq!(base.for_area(BodyArea::Chest, &worn, armor_of).armor, 30.0);
        assert_eq!(base.for_area(BodyArea::Head, &worn, armor_of).armor, 0.0);
        // The vest stuffed in the head slot guards nothing.
        let mut wrong = crate::item::Equipment::default();
        wrong.set(crate::item::EquipSlot::Head, worn.get(crate::item::EquipSlot::Chest).cloned());
        assert_eq!(base.for_area(BodyArea::Head, &wrong, armor_of).armor, 0.0);
    }

    #[test]
    fn pellets_spread_inside_the_cone_and_repeat_by_seed() {
        let aim = glam::Vec3::NEG_Z;
        let one = pellet_directions(aim, 0.0, 1, 7);
        assert_eq!(one, vec![aim]);
        let a = pellet_directions(aim, 5.0, 8, 7);
        let b = pellet_directions(aim, 5.0, 8, 7);
        assert_eq!(a, b, "same seed, same pattern");
        assert_ne!(a, pellet_directions(aim, 5.0, 8, 8), "another seed differs");
        for d in &a {
            let angle = d.angle_between(aim).to_degrees();
            assert!(angle <= 5.0 + 1e-3, "{angle}");
            assert!((d.length() - 1.0).abs() < 1e-5);
        }
        assert!(a.iter().any(|d| d.angle_between(aim) > 0.0), "not all dead centre");
    }

    fn pistol() -> DamageDef {
        DamageDef {
            name: "pistol round".to_string(),
            amount: 40.0,
            kind: DamageType::Bullet,
            knockback: 1.0,
            ignores_armor: false,
            self_scale: 0.5,
            falloff: Falloff {
                full_range: 10.0,
                far_range: 50.0,
                far_fraction: 0.5,
            },
        }
    }

    fn hit(def: &DamageDef) -> Hit<'_> {
        Hit {
            def,
            self_inflicted: false,
            distance: 1.0,
            location_scale: 1.0,
        }
    }

    #[test]
    fn a_hit_takes_health_and_reports_it() {
        let pipe = pipe();
        let mut hp = Health::new(100.0);
        let r = resolve_hit(&hit(&pipe), &mut Protection::default(), &mut hp);
        assert_eq!(r.damage_dealt, 20.0);
        assert_eq!(r.armor_absorbed, 0.0);
        assert!(!r.killed);
        assert_eq!(hp.current, 80.0);
    }

    #[test]
    fn a_lethal_hit_kills_and_health_stops_at_zero() {
        let pistol = pistol();
        let mut hp = Health::new(30.0);
        let r = resolve_hit(&hit(&pistol), &mut Protection::default(), &mut hp);
        assert!(r.killed);
        assert_eq!(hp.current, 0.0);
    }

    #[test]
    fn armor_absorbs_its_share_until_spent_quake_style() {
        let pistol = pistol();
        let mut hp = Health::new(100.0);
        let mut armor = Protection {
            armor: 20.0,
            resistances: vec![],
        };
        // ceil(40 * 0.66) = 27, capped at the 20 carried.
        let r = resolve_hit(&hit(&pistol), &mut armor, &mut hp);
        assert_eq!(r.armor_absorbed, 20.0);
        assert_eq!(r.damage_dealt, 20.0);
        assert_eq!(armor.armor, 0.0);
        assert_eq!(hp.current, 80.0);
        // Armor gone: the next hit lands whole.
        let r = resolve_hit(&hit(&pistol), &mut armor, &mut hp);
        assert_eq!(r.damage_dealt, 40.0);
    }

    #[test]
    fn falloff_scales_by_distance_rust_style() {
        let pistol = pistol();
        assert_eq!(pistol.falloff.at(5.0), 1.0);
        assert!((pistol.falloff.at(30.0) - 0.75).abs() < 1e-6);
        assert_eq!(pistol.falloff.at(80.0), 0.5);
        let mut hp = Health::new(100.0);
        let far = Hit {
            distance: 80.0,
            ..hit(&pistol)
        };
        let r = resolve_hit(&far, &mut Protection::default(), &mut hp);
        assert_eq!(r.damage_dealt, 20.0);
    }

    #[test]
    fn resistance_and_location_multiply() {
        let pipe = pipe();
        let mut hp = Health::new(100.0);
        let mut skeleton = Protection {
            armor: 0.0,
            resistances: vec![(DamageType::Blunt, 2.0)],
        };
        let head = Hit {
            location_scale: 2.0,
            ..hit(&pipe)
        };
        let r = resolve_hit(&head, &mut skeleton, &mut hp);
        assert_eq!(r.damage_dealt, 80.0);
    }

    #[test]
    fn self_damage_is_scaled_after_knockback_so_rocket_jumps_work() {
        let pipe = pipe();
        let mut hp = Health::new(100.0);
        let own = Hit {
            self_inflicted: true,
            ..hit(&pipe)
        };
        let r = resolve_hit(&own, &mut Protection::default(), &mut hp);
        assert_eq!(r.damage_dealt, 10.0);
        assert!((r.knockback - 20.0 / KNOCKBACK_MASS).abs() < 1e-6, "full knockback");
    }

    #[test]
    fn damage_never_drops_below_one() {
        let mut weak = pipe();
        weak.amount = 0.1;
        let mut hp = Health::new(10.0);
        let r = resolve_hit(&hit(&weak), &mut Protection::default(), &mut hp);
        assert_eq!(r.damage_dealt, 1.0);
    }

    #[test]
    fn pellets_sum_per_target() {
        let mut blast = MultiDamage::default();
        blast.add("zombie", 4.0);
        blast.add("zombie", 6.0);
        blast.add("wall", 5.0);
        let mut hits = blast.drain();
        hits.sort_by(|a, b| a.0.cmp(b.0));
        assert_eq!(hits, vec![("wall", 1, 5.0), ("zombie", 2, 5.0)]);
        assert!(blast.drain().is_empty());
    }

    #[test]
    fn the_fire_timer_gates_shots_and_charges_for_empty_clicks() {
        let mut timer = FireTimer::default();
        assert_eq!(timer.pull(0.4, true), Trigger::Fire);
        assert_eq!(timer.pull(0.4, true), Trigger::Waiting);
        timer.tick(0.3);
        assert_eq!(timer.pull(0.4, true), Trigger::Waiting);
        timer.tick(0.1);
        assert_eq!(timer.pull(0.4, false), Trigger::Empty);
        assert_eq!(timer.ready_in, EMPTY_DELAY);
    }

    #[test]
    fn damage_defs_register_once() {
        let mut reg = DamageRegistry::default();
        reg.register(pipe()).unwrap();
        assert!(reg.register(pipe()).is_err());
        assert_eq!(reg.def("pipe swing").unwrap().amount, 20.0);
        assert!(reg.def("nothing").is_none());
    }
}
