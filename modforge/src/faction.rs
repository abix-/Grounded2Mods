//! Factions as data (topside design.md "Factions and relationships"):
//! who exists, what kind they are, and how any two stand to each
//! other. Neutrals belong to no faction (operator-locked: "neutral
//! npcs shouldnt be part of factions"), so an actor's faction is an
//! `Option`. The one question the rest of the game asks is
//! `relation(a, b)`; what changes a relation (kills, trade, raids)
//! lands with the acts.

use std::collections::HashMap;

/// Return the first destination with capacity and an anchor within
/// reach of the arrival.
pub fn first_reachable_destination<'a>(
    arrival: (f32, f32),
    reach: f32,
    destinations: impl IntoIterator<Item = (usize, usize, &'a [(f32, f32)])>,
) -> Option<usize> {
    let reach_squared = reach * reach;
    destinations
        .into_iter()
        .find(|(_, capacity, anchors)| {
            *capacity > 0
                && anchors.iter().any(|anchor| {
                    let dx = arrival.0 - anchor.0;
                    let dy = arrival.1 - anchor.1;
                    dx * dx + dy * dy <= reach_squared
                })
        })
        .map(|(index, _, _)| index)
}

/// What a faction wants (design.md): hostile ones want your stuff,
/// builders want to grow, and the player's is yours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FactionKind {
    Hostile,
    Builder,
    Player,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FactionDef {
    pub name: String,
    pub kind: FactionKind,
}

/// How two factions stand to each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    Hostile,
    Neutral,
    Friendly,
}

/// The registered factions and the table between them.
#[derive(Debug, Default)]
pub struct FactionRegistry {
    defs: Vec<FactionDef>,
    /// Pairs stored with the names in sorted order, so (a, b) and
    /// (b, a) are one entry.
    relations: HashMap<(String, String), Relation>,
}

impl FactionRegistry {
    /// Register a faction. Its starting relations follow from kind:
    /// hostile factions are hostile to everyone; everyone else starts
    /// neutral.
    pub fn register(&mut self, def: FactionDef) -> Result<(), String> {
        if self.defs.iter().any(|d| d.name == def.name) {
            return Err(format!("faction '{}' registered twice", def.name));
        }
        for other in &self.defs {
            let relation = if def.kind == FactionKind::Hostile || other.kind == FactionKind::Hostile
            {
                Relation::Hostile
            } else {
                Relation::Neutral
            };
            self.relations
                .insert(Self::key(&def.name, &other.name), relation);
        }
        self.defs.push(def);
        Ok(())
    }

    pub fn def(&self, name: &str) -> Option<&FactionDef> {
        self.defs.iter().find(|d| d.name == name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.defs.iter().map(|d| d.name.as_str())
    }

    fn key(a: &str, b: &str) -> (String, String) {
        if a <= b {
            (a.to_string(), b.to_string())
        } else {
            (b.to_string(), a.to_string())
        }
    }

    /// How `a` stands to `b`. A neutral (None) is neutral to everyone
    /// and everyone to them; a faction is friendly to itself; two
    /// registered factions read the table.
    pub fn relation(&self, a: Option<&str>, b: Option<&str>) -> Relation {
        match (a, b) {
            (Some(a), Some(b)) if a == b => Relation::Friendly,
            (Some(a), Some(b)) => self
                .relations
                .get(&Self::key(a, b))
                .copied()
                .unwrap_or(Relation::Neutral),
            _ => Relation::Neutral,
        }
    }

    /// Set how two factions stand. Errors on an unregistered name.
    pub fn set_relation(&mut self, a: &str, b: &str, relation: Relation) -> Result<(), String> {
        for name in [a, b] {
            if self.def(name).is_none() {
                return Err(format!("faction '{name}' is not registered"));
            }
        }
        if a != b {
            self.relations.insert(Self::key(a, b), relation);
        }
        Ok(())
    }

    /// Whether `a` attacks `b` on sight.
    pub fn hostile(&self, a: Option<&str>, b: Option<&str>) -> bool {
        self.relation(a, b) == Relation::Hostile
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> FactionRegistry {
        let mut reg = FactionRegistry::default();
        for (name, kind) in [
            ("player", FactionKind::Player),
            ("raiders", FactionKind::Hostile),
            ("warlords", FactionKind::Hostile),
            ("settlers", FactionKind::Builder),
        ] {
            reg.register(FactionDef {
                name: name.to_string(),
                kind,
            })
            .unwrap();
        }
        reg
    }

    #[test]
    fn hostile_factions_hate_everyone_and_builders_start_neutral() {
        let reg = registry();
        assert!(reg.hostile(Some("raiders"), Some("player")));
        assert!(reg.hostile(Some("player"), Some("raiders")), "both ways");
        assert!(
            reg.hostile(Some("raiders"), Some("warlords")),
            "hostiles fight each other"
        );
        assert_eq!(
            reg.relation(Some("settlers"), Some("player")),
            Relation::Neutral
        );
        assert_eq!(
            reg.relation(Some("raiders"), Some("raiders")),
            Relation::Friendly
        );
    }

    #[test]
    fn neutrals_belong_to_no_one_and_fight_no_one() {
        let reg = registry();
        assert_eq!(reg.relation(None, Some("raiders")), Relation::Neutral);
        assert_eq!(reg.relation(Some("raiders"), None), Relation::Neutral);
        assert_eq!(reg.relation(None, None), Relation::Neutral);
        assert!(!reg.hostile(Some("raiders"), None));
    }

    #[test]
    fn relations_can_change_and_unknown_names_are_refused() {
        let mut reg = registry();
        reg.set_relation("settlers", "player", Relation::Friendly)
            .unwrap();
        assert_eq!(
            reg.relation(Some("player"), Some("settlers")),
            Relation::Friendly
        );
        reg.set_relation("settlers", "player", Relation::Hostile)
            .unwrap();
        assert!(reg.hostile(Some("settlers"), Some("player")));
        assert!(
            reg.set_relation("settlers", "nobody", Relation::Hostile)
                .is_err()
        );
        assert!(
            reg.register(FactionDef {
                name: "raiders".to_string(),
                kind: FactionKind::Hostile
            })
            .is_err()
        );
    }
}
