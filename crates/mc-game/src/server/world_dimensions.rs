//! `world.level.levelgen.WorldDimensions` over JSON stems: the dimensions a world is created with (a `world_preset`,
//! the stored `world_gen_settings.dat`), baked with the data pack's own `dimension` entries into the final
//! `dimension` registry (`WorldDimensions.bake`).

use crate::registry::codec::{Lookup, RegistryDecodeContext, RegistryLookup};
use crate::registry::typed::level_stem::LevelStem;
use crate::registry::{Registry, RegistryAccess, RegistryEntry, RegistrationInfo, keys};
use mc_core::ident::Identifier;
use mc_nbt::{CompoundTag, Tag};
use mc_world::level_storage::SpecialWorldProperty;
use serde_json::{Map, Number, Value};
use std::sync::Arc;

/// `WorldDimensions`: dimension key -> `LevelStem` JSON (`{"type": .., "generator": ..}`), in insertion order.
#[derive(Clone, Debug, Default)]
pub struct WorldDimensions {
    pub dimensions: Vec<(Identifier, Value)>,
}

pub fn overworld_key() -> Identifier {
    overworld()
}

/// `Level.NETHER`.
pub fn nether_key() -> Identifier {
    Identifier::with_default_namespace("the_nether").expect("valid id")
}

/// `Level.END`.
pub fn end_key() -> Identifier {
    Identifier::with_default_namespace("the_end").expect("valid id")
}

fn overworld() -> Identifier {
    Identifier::with_default_namespace("overworld").expect("valid id")
}

/// `WorldDimensions.keysInOrder`: overworld, nether, end when known, then the others in the given order.
pub fn keys_in_order(known: &[Identifier]) -> Vec<Identifier> {
    let builtin = ["overworld", "the_nether", "the_end"].map(|p| Identifier::with_default_namespace(p).expect("valid id"));
    let mut out: Vec<Identifier> = builtin.iter().filter(|k| known.contains(k)).cloned().collect();
    out.extend(known.iter().filter(|k| !builtin.contains(k)).cloned());
    out
}

impl WorldDimensions {
    /// `WorldPreset.createWorldDimensions`: the preset's `dimensions` object, `dimensionsInOrder`.
    pub fn from_preset_json(preset: &Value) -> Result<WorldDimensions, String> {
        let dimensions = preset.get("dimensions").and_then(Value::as_object).ok_or("No key dimensions")?;
        let mut parsed = Vec::new();
        for (key, stem) in dimensions {
            parsed.push((Identifier::read(key)?, stem.clone()));
        }
        let keys: Vec<Identifier> = parsed.iter().map(|(k, _)| k.clone()).collect();
        let ordered = keys_in_order(&keys).into_iter().filter_map(|k| parsed.iter().find(|(d, _)| *d == k).cloned()).collect();
        let world = WorldDimensions { dimensions: ordered };
        if world.get(&overworld()).is_none() {
            return Err("Missing overworld dimension".to_owned());
        }
        Ok(world)
    }

    /// `WorldGenSettings.dimensions` (the stored compound of stems).
    pub fn from_stored(dimensions: &CompoundTag) -> Result<WorldDimensions, String> {
        let mut parsed = Vec::new();
        for key in dimensions.keys() {
            let stem = dimensions.get(key).expect("key of the compound");
            parsed.push((Identifier::read(key)?, nbt_to_json(stem)));
        }
        let world = WorldDimensions { dimensions: parsed };
        if world.get(&overworld()).is_none() {
            return Err("Overworld settings missing".to_owned());
        }
        Ok(world)
    }

    pub fn get(&self, key: &Identifier) -> Option<&Value> {
        self.dimensions.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// `replaceOverworldGenerator`: the overworld stem keeps its dimension type and gets `generator`.
    pub fn replace_overworld_generator(&mut self, generator: Value) {
        if let Some((_, stem)) = self.dimensions.iter_mut().find(|(k, _)| *k == overworld())
            && let Some(object) = stem.as_object_mut()
        {
            object.insert("generator".to_owned(), generator);
        }
    }

    /// `WorldGenSettings.dimensions` as stored in `world_gen_settings.dat`.
    pub fn to_stored(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        for (key, stem) in &self.dimensions {
            tag.put(&key.to_string(), crate::registry::codec::json_to_nbt(stem));
        }
        tag
    }

    /// `specialWorldProperty` of the overworld generator type.
    pub fn special_world_property(&self) -> SpecialWorldProperty {
        match self.get(&overworld()).and_then(|stem| stem.get("generator")).and_then(|g| g.get("type")).and_then(Value::as_str) {
            Some("minecraft:debug") => SpecialWorldProperty::Debug,
            Some("minecraft:flat") => SpecialWorldProperty::Flat,
            _ => SpecialWorldProperty::None,
        }
    }

    /// `bake(baseDimensions)`: for every known key, the data pack's stem if there is one, else this world's. Returns
    /// the final `dimension` registry and the special world property of the result.
    pub fn bake(&self, base_dimensions: Option<&Registry>) -> Result<(RegistryAccess, SpecialWorldProperty), String> {
        let mut known: Vec<Identifier> = base_dimensions.map(Registry::names).unwrap_or_default();
        for (key, _) in &self.dimensions {
            if !known.contains(key) {
                known.push(key.clone());
            }
        }
        let lookup = AnyLookup;
        let ctx = RegistryDecodeContext::new(&lookup);
        let mut entries = Vec::new();
        let mut special = SpecialWorldProperty::None;
        for key in keys_in_order(&known) {
            let (json, info) = match base_dimensions.and_then(|r| r.entry_by_key(&key)) {
                Some(entry) => (entry.json.clone(), entry.info.clone()),
                None => match self.get(&key) {
                    Some(json) => (Arc::new(json.clone()), RegistrationInfo { known_pack: None, stable: true }),
                    None => continue,
                },
            };
            let stem = LevelStem::from_json(&json, &ctx).map_err(|e| format!("Failed to parse dimension {key}: {e}"))?;
            if key == overworld() {
                special = match stem.generator_type() {
                    "minecraft:debug" => SpecialWorldProperty::Debug,
                    "minecraft:flat" => SpecialWorldProperty::Flat,
                    _ => SpecialWorldProperty::None,
                };
            }
            entries.push(RegistryEntry { id: key, json, value: Some(Arc::new(stem)), network: None, source_pack: "world".to_owned(), info });
        }
        Ok((RegistryAccess::from_registries([Registry::dynamic(keys::LEVEL_STEM, entries)]), special))
    }
}

/// Every reference exists: stems of a world were validated when they were created.
struct AnyLookup;

impl RegistryLookup for AnyLookup {
    fn contains(&self, _registry: &str, _id: &Identifier) -> Lookup {
        Lookup::Yes
    }
    fn tag_exists(&self, _registry: &str, _tag: &Identifier) -> Lookup {
        Lookup::Yes
    }
}

/// `NbtOps` -> `JsonOps` (`Dynamic.convert`): compounds/lists/strings/numbers. Integral tags become JSON integers,
/// floating tags floats.
pub fn nbt_to_json(tag: &Tag) -> Value {
    match tag {
        Tag::Compound(c) => {
            let mut map = Map::new();
            for key in c.keys() {
                map.insert(key.to_string(), nbt_to_json(c.get(key).expect("key of the compound")));
            }
            Value::Object(map)
        }
        Tag::List(items) => Value::Array(items.iter().map(nbt_to_json).collect()),
        Tag::String(s) => Value::String(s.to_string()),
        Tag::Byte(v) => Value::Number((*v).into()),
        Tag::Short(v) => Value::Number((*v).into()),
        Tag::Int(v) => Value::Number((*v).into()),
        Tag::Long(v) => Value::Number((*v).into()),
        Tag::Float(v) => Number::from_f64(f64::from(*v)).map_or(Value::Null, Value::Number),
        Tag::Double(v) => Number::from_f64(*v).map_or(Value::Null, Value::Number),
        other => Value::String(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preset_dimensions_are_ordered_and_roundtrip_through_nbt() {
        let preset = json!({"dimensions": {
            "minecraft:the_end": {"type": "minecraft:the_end", "generator": {"type": "minecraft:noise"}},
            "minecraft:overworld": {"type": "minecraft:overworld", "generator": {"type": "minecraft:flat", "settings": {"layers": [{"block": "minecraft:stone", "height": 1}]}}},
            "custom:x": {"type": "minecraft:overworld", "generator": {"type": "minecraft:noise"}},
            "minecraft:the_nether": {"type": "minecraft:the_nether", "generator": {"type": "minecraft:noise"}},
        }});
        let dims = WorldDimensions::from_preset_json(&preset).unwrap();
        let names: Vec<String> = dims.dimensions.iter().map(|(k, _)| k.to_string()).collect();
        assert_eq!(names, ["minecraft:overworld", "minecraft:the_nether", "minecraft:the_end", "custom:x"]);
        assert_eq!(dims.special_world_property(), SpecialWorldProperty::Flat);
        let stored = dims.to_stored();
        let back = WorldDimensions::from_stored(&stored).unwrap();
        assert_eq!(back.get(&overworld()), dims.get(&overworld()));
        assert!(WorldDimensions::from_preset_json(&json!({"dimensions": {"minecraft:the_end": {}}})).is_err());
    }

    #[test]
    fn replace_generator_keeps_type() {
        let mut dims = WorldDimensions::from_preset_json(&json!({"dimensions": {"minecraft:overworld": {"type": "minecraft:overworld", "generator": {"type": "minecraft:noise"}}}})).unwrap();
        dims.replace_overworld_generator(json!({"type": "minecraft:flat", "settings": {}}));
        assert_eq!(dims.get(&overworld()).unwrap()["type"], "minecraft:overworld");
        assert_eq!(dims.special_world_property(), SpecialWorldProperty::Flat);
    }
}
