//! The positional biome layer of the `gameplay/natural_mob_spawns` and `gameplay/creature_world_gen_spawn_probability`
//! environment attributes (`EnvironmentAttributeSystem.addBiomeLayer`), typed.
//!
//! The worldgen biome registry keeps the biome `attributes` as raw JSON; [`BiomeSpawnTable`] decodes the two spawning
//! attributes of every biome once per registry (memoized by [`BiomeSpawnTable::for_registry`], shared by the server level
//! and the generation threads). [`resolve`] evaluates the attribute stack of a position: the dimension type's constant
//! layer, then the biome layer (the attribute is `fullResolutionBiomes`, so the biome is the block-resolution
//! `BiomeManager.getBiome`).

use super::mob_spawn_settings::{
    DEFAULT_CREATURE_WORLD_GEN_SPAWN_PROBABILITY, EMPTY, MobSpawnSettings,
};
use crate::registry::typed::attributes::EnvironmentAttributeMap;
use mc_core::ident::Identifier;
use mc_core::math::mth;
use mc_world::palette::BiomeId;
use mc_worldgen::biome::BiomeRegistry;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, Weak};

const NATURAL_MOB_SPAWNS: &str = "minecraft:gameplay/natural_mob_spawns";
const CREATURE_PROBABILITY: &str = "minecraft:gameplay/creature_world_gen_spawn_probability";
/// `EnvironmentAttributes.CREATURE_WORLD_GEN_SPAWN_PROBABILITY` range (`AttributeRange.UNIT_FLOAT_EPSILON`).
const PROBABILITY_MAX: f32 = 0.9999999;

/// One `natural_mob_spawns` layer entry: the `overlay` modifier or a plain/`override` value.
#[derive(Clone, Debug)]
pub struct SpawnsEntry {
    pub overlay: bool,
    pub settings: Arc<MobSpawnSettings>,
}

impl SpawnsEntry {
    /// `applyModifier(baseValue)`.
    fn apply(&self, base: &Arc<MobSpawnSettings>) -> Arc<MobSpawnSettings> {
        if self.overlay {
            MobSpawnSettings::overlay(base, &self.settings)
        } else {
            self.settings.clone()
        }
    }

    /// An `EnvironmentAttributeMap` entry (`modifier` + `argument` as validated JSON).
    fn from_entry(modifier: &str, argument: &Value) -> Option<SpawnsEntry> {
        match MobSpawnSettings::from_json(argument) {
            Ok(settings) => Some(SpawnsEntry {
                overlay: modifier == "overlay",
                settings: Arc::new(settings),
            }),
            Err(e) => {
                log::warn!("Invalid {NATURAL_MOB_SPAWNS} attribute: {e}");
                None
            }
        }
    }
}

/// One float layer entry (`creature_world_gen_spawn_probability`).
#[derive(Clone, Copy, Debug)]
pub struct FloatEntry {
    modifier: FloatModifier,
    value: f32,
    alpha: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FloatModifier {
    Override,
    Add,
    Subtract,
    Multiply,
    Minimum,
    Maximum,
    AlphaBlend,
}

impl FloatEntry {
    fn from_entry(modifier: &str, argument: &Value) -> Option<FloatEntry> {
        let modifier = match modifier {
            "override" => FloatModifier::Override,
            "add" => FloatModifier::Add,
            "subtract" => FloatModifier::Subtract,
            "multiply" => FloatModifier::Multiply,
            "minimum" => FloatModifier::Minimum,
            "maximum" => FloatModifier::Maximum,
            "alpha_blend" => FloatModifier::AlphaBlend,
            _ => return None,
        };
        if modifier == FloatModifier::AlphaBlend {
            let object = argument.as_object()?;
            let value = object.get("value")?.as_f64()? as f32;
            let alpha = object
                .get("alpha")
                .map_or(Some(1.0), |a| a.as_f64().map(|a| a as f32))?;
            return Some(FloatEntry {
                modifier,
                value,
                alpha,
            });
        }
        Some(FloatEntry {
            modifier,
            value: argument.as_f64()? as f32,
            alpha: 1.0,
        })
    }

    /// `FloatModifier.apply(subject, argument)`.
    fn apply(&self, subject: f32) -> f32 {
        match self.modifier {
            FloatModifier::Override => self.value,
            FloatModifier::Add => subject + self.value,
            FloatModifier::Subtract => subject - self.value,
            FloatModifier::Multiply => subject * self.value,
            FloatModifier::Minimum => subject.min(self.value),
            FloatModifier::Maximum => subject.max(self.value),
            FloatModifier::AlphaBlend => mth::lerp_f32(self.alpha, subject, self.value),
        }
    }
}

/// The constant layer of the dimension type (`addDimensionLayer`), applied to the attribute defaults.
#[derive(Clone, Debug)]
pub struct DimensionSpawnBase {
    pub natural_mob_spawns: Arc<MobSpawnSettings>,
    pub creature_probability: f32,
}

impl Default for DimensionSpawnBase {
    fn default() -> Self {
        DimensionSpawnBase {
            natural_mob_spawns: EMPTY.clone(),
            creature_probability: DEFAULT_CREATURE_WORLD_GEN_SPAWN_PROBABILITY,
        }
    }
}

impl DimensionSpawnBase {
    /// The baked constant base value of the two attributes for a dimension type's `attributes`.
    pub fn from_attributes(attributes: &EnvironmentAttributeMap) -> DimensionSpawnBase {
        let mut base = DimensionSpawnBase::default();
        if let Some(entry) = attributes.get(&attribute_id(NATURAL_MOB_SPAWNS))
            && let Some(entry) = SpawnsEntry::from_entry(&entry.modifier, &entry.argument)
        {
            base.natural_mob_spawns = entry.apply(&base.natural_mob_spawns);
        }
        if let Some(entry) = attributes.get(&attribute_id(CREATURE_PROBABILITY))
            && let Some(entry) = FloatEntry::from_entry(&entry.modifier, &entry.argument)
        {
            base.creature_probability = entry.apply(base.creature_probability);
        }
        base
    }
}

fn attribute_id(name: &str) -> Identifier {
    Identifier::parse(name).expect("valid attribute id")
}

/// Both spawning attributes of every biome of one [`BiomeRegistry`], indexed by [`BiomeId`].
#[derive(Debug, Default)]
pub struct BiomeSpawnTable {
    natural: Vec<Option<SpawnsEntry>>,
    creature_probability: Vec<Option<FloatEntry>>,
}

impl BiomeSpawnTable {
    /// Decodes the attributes of every biome of the registry.
    pub fn build(registry: &BiomeRegistry) -> BiomeSpawnTable {
        let mut table = BiomeSpawnTable::default();
        for index in 0..registry.len() {
            let attributes = registry.get(BiomeId(index as u32)).attributes.as_ref();
            table.natural.push(
                attributes
                    .and_then(|a| entry_of(a, NATURAL_MOB_SPAWNS))
                    .and_then(|(m, v)| SpawnsEntry::from_entry(m, v)),
            );
            table.creature_probability.push(
                attributes
                    .and_then(|a| entry_of(a, CREATURE_PROBABILITY))
                    .and_then(|(m, v)| FloatEntry::from_entry(m, v)),
            );
        }
        table
    }

    /// The table of `registry`, built on first use and shared afterwards (one table per registry instance).
    pub fn for_registry(registry: &Arc<BiomeRegistry>) -> Arc<BiomeSpawnTable> {
        type TableCache = Mutex<HashMap<usize, (Weak<BiomeRegistry>, Arc<BiomeSpawnTable>)>>;
        static TABLES: LazyLock<TableCache> = LazyLock::new(Mutex::default);
        let mut tables = TABLES.lock().unwrap_or_else(|e| e.into_inner());
        tables.retain(|_, (registry, _)| registry.strong_count() > 0);
        let key = Arc::as_ptr(registry) as usize;
        if let Some((_, table)) = tables.get(&key) {
            return table.clone();
        }
        let table = Arc::new(BiomeSpawnTable::build(registry));
        tables.insert(key, (Arc::downgrade(registry), table.clone()));
        table
    }

    /// `getValue(NATURAL_MOB_SPAWNS, pos)` for the biome at `pos`.
    pub fn natural_mob_spawns(
        &self,
        base: &DimensionSpawnBase,
        biome: Option<BiomeId>,
    ) -> Arc<MobSpawnSettings> {
        match biome
            .and_then(|b| self.natural.get(b.0 as usize))
            .and_then(Option::as_ref)
        {
            Some(entry) => entry.apply(&base.natural_mob_spawns),
            None => base.natural_mob_spawns.clone(),
        }
    }

    /// `getValue(CREATURE_WORLD_GEN_SPAWN_PROBABILITY, pos)` for the biome at `pos` (sanitized to the attribute range).
    pub fn creature_world_gen_spawn_probability(
        &self,
        base: &DimensionSpawnBase,
        biome: Option<BiomeId>,
    ) -> f32 {
        let value = match biome
            .and_then(|b| self.creature_probability.get(b.0 as usize))
            .and_then(Option::as_ref)
        {
            Some(entry) => entry.apply(base.creature_probability),
            None => base.creature_probability,
        };
        if (0.0..=PROBABILITY_MAX).contains(&value) {
            value
        } else {
            mth::clamp_f32(value, 0.0, PROBABILITY_MAX)
        }
    }
}

/// The `(modifier, argument)` of an attribute in a biome's raw `attributes` object (`Codec.either(valueCodec, fullCodec)`:
/// an object with `modifier` and `argument` is the full form, anything else a plain override value).
fn entry_of<'a>(attributes: &'a Value, name: &str) -> Option<(&'a str, &'a Value)> {
    let value = attributes.get(name)?;
    if let Some(object) = value.as_object()
        && let (Some(modifier), Some(argument)) = (
            object.get("modifier").and_then(Value::as_str),
            object.get("argument"),
        )
    {
        return Some((modifier, argument));
    }
    Some(("override", value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn entry_forms() {
        let attributes = json!({
            NATURAL_MOB_SPAWNS: {"modifier": "overlay", "argument": {"spawns_by_category": {}, "spawn_costs": {}}},
            CREATURE_PROBABILITY: 0.03
        });
        assert_eq!(
            entry_of(&attributes, NATURAL_MOB_SPAWNS).map(|e| e.0),
            Some("overlay")
        );
        assert_eq!(
            entry_of(&attributes, CREATURE_PROBABILITY).map(|e| e.0),
            Some("override")
        );
        let entry = FloatEntry::from_entry("override", &json!(0.03)).unwrap();
        assert_eq!(entry.apply(0.1), 0.03f32);
        assert_eq!(
            FloatEntry::from_entry("add", &json!(0.5))
                .unwrap()
                .apply(0.25),
            0.75
        );
    }
}
