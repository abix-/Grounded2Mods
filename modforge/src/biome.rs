//! The biome vocabulary: a biome as DATA. A BiomeDef is the generic
//! definition of a type of area (topside design.md, operator-locked:
//! biomes control weather, possible monuments, possible npcs,
//! wildlife, and probably more); a generator builds concrete areas
//! from it. Plain glam math and rgb, no engine types, like
//! [`crate::structure`].
//!
//! The lists name defs held by other registries (weather kinds,
//! MonumentDefs, npc kinds, wildlife kinds, ItemDefs); a biome only
//! says what is POSSIBLE here, never places anything itself.

use crate::structure::Rgb;
use glam::Vec3;

/// One kind of thing nature strews across a biome (trees, rocks,
/// wrecks), with how densely.
#[derive(Clone)]
pub struct ScatterSpec {
    pub size: Vec3,
    pub color: Rgb,
    pub density: f32,
}

/// The generic definition of a type of area. `name` is the id. The
/// generator builds concrete areas from this; the storm re-rolls the
/// built areas, never the defs.
#[derive(Clone)]
pub struct BiomeDef {
    pub name: String,
    pub ground: Rgb,
    pub scatter: Vec<ScatterSpec>,
    pub weather: Vec<String>,
    pub monuments: Vec<String>,
    pub npcs: Vec<String>,
    pub wildlife: Vec<String>,
    pub harvest: Vec<String>,
}

/// The collection of checked-in BiomeDefs. Consumers register their
/// content at startup and look defs up by name.
#[derive(Default)]
pub struct BiomeRegistry {
    defs: Vec<BiomeDef>,
}

impl BiomeRegistry {
    pub fn register(&mut self, def: BiomeDef) -> Result<(), String> {
        if self.defs.iter().any(|d| d.name == def.name) {
            return Err(format!("biome '{}' registered twice", def.name));
        }
        self.defs.push(def);
        Ok(())
    }

    pub fn def(&self, name: &str) -> Option<&BiomeDef> {
        self.defs.iter().find(|d| d.name == name)
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str) -> BiomeDef {
        BiomeDef {
            name: name.to_string(),
            ground: [0.2, 0.4, 0.2],
            scatter: vec![ScatterSpec {
                size: Vec3::new(0.5, 3.0, 0.5),
                color: [0.3, 0.25, 0.15],
                density: 0.02,
            }],
            weather: vec!["rain".to_string()],
            monuments: vec![],
            npcs: vec![],
            wildlife: vec!["deer".to_string()],
            harvest: vec!["scrap".to_string()],
        }
    }

    #[test]
    fn registry_serves_defs_and_rejects_duplicates() {
        let mut reg = BiomeRegistry::default();
        reg.register(def("forest")).unwrap();
        assert_eq!(reg.def("forest").unwrap().wildlife, vec!["deer"]);
        assert!(reg.def("wasteland").is_none());
        assert!(reg.register(def("forest")).is_err());
    }
}
