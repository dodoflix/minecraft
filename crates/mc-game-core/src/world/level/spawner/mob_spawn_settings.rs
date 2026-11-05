//! `net.minecraft.world.level.biome.MobSpawnSettings` (typed): the spawner lists per `MobCategory` and the per-entity-type
//! spawn costs of the `gameplay/natural_mob_spawns` environment attribute, plus the `Overlay` modifier that merges two
//! settings (`MobSpawnSettingsModifier.Overlay`).
//!
//! `SpawnerData` (`type` + `count`) is the one the structure spawn overrides use
//! ([`SpawnerData`](crate::world::level::levelgen::structure::structure::SpawnerData)); weights live in the
//! [`WeightedList`] entries.

use crate::world::level::levelgen::structure::structure::{SpawnerData, mob_category_by_name};
use mc_data::enums::MobCategory;
use mc_data::ids::EntityType;
use mc_worldgen::provider::{IntProvider, Weighted, WeightedList};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

/// `MobSpawnSettings.DEFAULT_CREATURE_WORLD_GEN_SPAWN_PROBABILITY`.
pub const DEFAULT_CREATURE_WORLD_GEN_SPAWN_PROBABILITY: f32 = 0.1;

/// `MobSpawnSettings.EMPTY_MOB_LIST`.
pub static EMPTY_MOB_LIST: LazyLock<WeightedList<SpawnerData>> = LazyLock::new(|| WeightedList::of(Vec::new()).expect("empty list"));

/// `MobSpawnSettings.MobSpawnCost(energyBudget, charge)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MobSpawnCost {
    pub energy_budget: f64,
    pub charge: f64,
}

/// `MobSpawnSettings`.
#[derive(Clone, Debug, Default)]
pub struct MobSpawnSettings {
    /// `spawnsByCategory`, indexed by `MobCategory` ordinal (`None` = category not defined by these settings).
    spawns_by_category: [Option<WeightedList<SpawnerData>>; 8],
    /// `mobSpawnCosts`.
    mob_spawn_costs: HashMap<EntityType, MobSpawnCost>,
}

/// `MobSpawnSettings.EMPTY`.
pub static EMPTY: LazyLock<Arc<MobSpawnSettings>> = LazyLock::new(|| Arc::new(MobSpawnSettings::default()));

fn index(category: MobCategory) -> usize {
    category as usize
}

impl MobSpawnSettings {
    /// `getMobsToSpawn(category)`.
    pub fn get_mobs_to_spawn(&self, category: MobCategory) -> &WeightedList<SpawnerData> {
        self.spawns_by_category[index(category)].as_ref().unwrap_or(&EMPTY_MOB_LIST)
    }

    /// `getMobsInCategory(category)`.
    pub fn get_mobs_in_category(&self, category: MobCategory) -> Option<&WeightedList<SpawnerData>> {
        self.spawns_by_category[index(category)].as_ref()
    }

    /// `definedCategories()`.
    pub fn defined_categories(&self) -> impl Iterator<Item = MobCategory> + '_ {
        MobCategory::ALL.iter().copied().filter(|c| self.spawns_by_category[index(*c)].is_some())
    }

    /// `getMobSpawnCost(type)`.
    pub fn get_mob_spawn_cost(&self, ty: EntityType) -> Option<&MobSpawnCost> {
        self.mob_spawn_costs.get(&ty)
    }

    /// `allSpawnCosts()`.
    pub fn all_spawn_costs(&self) -> &HashMap<EntityType, MobSpawnCost> {
        &self.mob_spawn_costs
    }

    /// `Builder`-built settings: `spawns` per category (`None` = not defined), `costs` in insertion order.
    pub fn new(spawns_by_category: [Option<WeightedList<SpawnerData>>; 8], mob_spawn_costs: HashMap<EntityType, MobSpawnCost>) -> MobSpawnSettings {
        MobSpawnSettings { spawns_by_category, mob_spawn_costs }
    }

    /// `MobSpawnSettings.CODEC` (`spawns_by_category` + `spawn_costs`, both required).
    pub fn from_json(value: &Value) -> Result<MobSpawnSettings, String> {
        let object = value.as_object().ok_or_else(|| format!("Not a map: {value}"))?;
        let mut settings = MobSpawnSettings::default();
        let by_category = object.get("spawns_by_category").and_then(Value::as_object).ok_or("No key spawns_by_category")?;
        for (key, list) in by_category {
            let category = mob_category_by_name(key).ok_or_else(|| format!("Unknown element name:{key}"))?;
            let entries = list.as_array().ok_or_else(|| format!("Not a list: {list}"))?;
            let mut items = Vec::with_capacity(entries.len());
            for entry in entries {
                items.push(parse_spawner(entry)?);
            }
            settings.spawns_by_category[index(category)] = Some(WeightedList::of(items).map_err(|e| e.to_string())?);
        }
        let costs = object.get("spawn_costs").and_then(Value::as_object).ok_or("No key spawn_costs")?;
        for (key, cost) in costs {
            let ty = EntityType::by_name(key).ok_or_else(|| format!("Unknown registry key: {key}"))?;
            let energy_budget = cost.get("energy_budget").and_then(Value::as_f64).ok_or("No key energy_budget")?;
            let charge = cost.get("charge").and_then(Value::as_f64).ok_or("No key charge")?;
            settings.mob_spawn_costs.insert(ty, MobSpawnCost { energy_budget, charge });
        }
        Ok(settings)
    }

    /// `MobSpawnSettingsModifier.Overlay.apply(first, second)`.
    pub fn overlay(first: &Arc<MobSpawnSettings>, second: &Arc<MobSpawnSettings>) -> Arc<MobSpawnSettings> {
        let first_defined = first.defined_categories().count();
        let second_defined = second.defined_categories().count();
        if first_defined == 0 && first.mob_spawn_costs.is_empty() {
            return second.clone();
        }
        if second_defined == 0 && second.mob_spawn_costs.is_empty() {
            return first.clone();
        }
        let second_covers_categories = first.defined_categories().all(|c| second.spawns_by_category[index(c)].is_some());
        if second_covers_categories && first.mob_spawn_costs.keys().all(|k| second.mob_spawn_costs.contains_key(k)) {
            return second.clone();
        }
        let mut result = MobSpawnSettings::default();
        for category in MobCategory::ALL {
            let i = index(*category);
            result.spawns_by_category[i] = second.spawns_by_category[i].clone().or_else(|| first.spawns_by_category[i].clone());
        }
        result.mob_spawn_costs.extend(first.mob_spawn_costs.iter().map(|(k, v)| (*k, *v)));
        result.mob_spawn_costs.extend(second.mob_spawn_costs.iter().map(|(k, v)| (*k, *v)));
        Arc::new(result)
    }

    /// `FORTRESS_ENEMIES` (`NetherFortressStructure`).
    pub fn fortress_enemies() -> &'static WeightedList<SpawnerData> {
        static FORTRESS: LazyLock<WeightedList<SpawnerData>> = LazyLock::new(|| {
            let entry = |name: &str, count: IntProvider, weight: i32| Weighted { value: SpawnerData { entity_type: EntityType::by_name(name).expect("entity type"), count }, weight };
            WeightedList::of(vec![
                entry("minecraft:blaze", IntProvider::Uniform { min_inclusive: 2, max_inclusive: 3 }, 10),
                entry("minecraft:zombified_piglin", IntProvider::Constant(4), 5),
                entry("minecraft:wither_skeleton", IntProvider::Constant(5), 8),
                entry("minecraft:skeleton", IntProvider::Constant(5), 2),
                entry("minecraft:magma_cube", IntProvider::Constant(4), 3),
            ])
            .expect("weights fit")
        });
        &FORTRESS
    }
}

/// One `WeightedList.codec(SpawnerData.CODEC)` element: `type`, `count` and `weight`.
fn parse_spawner(entry: &Value) -> Result<Weighted<SpawnerData>, String> {
    let ty_name = entry.get("type").and_then(Value::as_str).ok_or("No key type")?;
    let mut entity_type = EntityType::by_name(ty_name).ok_or_else(|| format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:entity_type]: {ty_name}"))?;
    let count = IntProvider::from_json(entry.get("count").ok_or("No key count")?).map_err(|e| e.to_string())?;
    if entity_type.data().category == MobCategory::Misc {
        // `SpawnerData` canonical constructor: MISC types become pigs
        entity_type = EntityType::by_name("minecraft:pig").ok_or("pig")?;
    }
    let weight = entry.get("weight").map_or(Ok(1), |w| w.as_i64().and_then(|w| i32::try_from(w).ok()).ok_or("weight is not an int"))?;
    if weight < 0 {
        return Err(format!("Value must be non-negative: {weight}"));
    }
    Ok(Weighted { value: SpawnerData { entity_type, count }, weight })
}

/// `WeightedList.contains(data)`.
pub fn list_contains(list: &WeightedList<SpawnerData>, data: &SpawnerData) -> bool {
    list.unwrap().iter().any(|w| w.value == *data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decodes_and_overlays() {
        let base = Arc::new(
            MobSpawnSettings::from_json(&json!({
                "spawns_by_category": {"creature": [{"type": "minecraft:sheep", "count": 4, "weight": 12}, {"type": "minecraft:horse", "count": {"type": "minecraft:uniform", "min_inclusive": 2, "max_inclusive": 6}, "weight": 5}]},
                "spawn_costs": {"minecraft:enderman": {"energy_budget": 0.7, "charge": 0.15}}
            }))
            .unwrap(),
        );
        assert_eq!(base.get_mobs_to_spawn(MobCategory::Creature).unwrap().len(), 2);
        assert!(base.get_mobs_to_spawn(MobCategory::Monster).is_empty());
        assert_eq!(base.get_mob_spawn_cost(EntityType::ENDERMAN), Some(&MobSpawnCost { energy_budget: 0.7, charge: 0.15 }));
        let other = Arc::new(MobSpawnSettings::from_json(&json!({"spawns_by_category": {"monster": []}, "spawn_costs": {}})).unwrap());
        let merged = MobSpawnSettings::overlay(&base, &other);
        assert!(merged.get_mobs_in_category(MobCategory::Creature).is_some());
        assert!(merged.get_mobs_in_category(MobCategory::Monster).is_some());
        assert!(merged.get_mob_spawn_cost(EntityType::ENDERMAN).is_some());
        assert!(Arc::ptr_eq(&MobSpawnSettings::overlay(&EMPTY, &base), &base));
        assert!(Arc::ptr_eq(&MobSpawnSettings::overlay(&base, &EMPTY), &base));
    }
}
