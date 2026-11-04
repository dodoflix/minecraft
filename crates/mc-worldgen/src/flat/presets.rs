//! `FlatLevelGeneratorPreset` and the vanilla `worldgen/flat_level_generator_preset` entries
//! (`FlatLevelGeneratorPresets`; the data pack JSON is the output of its `Bootstrap`: layers bottom to top).

use std::sync::Arc;

use mc_core::ident::Identifier;
use mc_data::Item;
use serde_json::Value;

use super::settings::FlatLevelGeneratorSettings;
use crate::biome::BiomeRegistry;
use crate::data::{LoadError, WorldgenRegistries};
use crate::densityfunction::codec::{Fields, LoadResult, parse_identifier};

pub const FLAT_LEVEL_GENERATOR_PRESET_REGISTRY: &str = "worldgen/flat_level_generator_preset";

/// Keys of `FlatLevelGeneratorPresets` (`TEST_WORLD` is registered as a key but has no entry in vanilla).
pub const CLASSIC_FLAT: &str = "classic_flat";
pub const TUNNELERS_DREAM: &str = "tunnelers_dream";
pub const WATER_WORLD: &str = "water_world";
pub const OVERWORLD: &str = "overworld";
pub const SNOWY_KINGDOM: &str = "snowy_kingdom";
pub const BOTTOMLESS_PIT: &str = "bottomless_pit";
pub const DESERT: &str = "desert";
pub const REDSTONE_READY: &str = "redstone_ready";
pub const TEST_WORLD: &str = "test_world";
pub const THE_VOID: &str = "the_void";

#[derive(Clone, Debug)]
pub struct FlatLevelGeneratorPreset {
    pub display_item: Item,
    pub settings: FlatLevelGeneratorSettings,
}

impl FlatLevelGeneratorPreset {
    /// `FlatLevelGeneratorPreset.DIRECT_CODEC`: `display` (a non-air item id) and `settings`.
    pub fn from_json(
        value: &Value,
        registries: &WorldgenRegistries,
        biomes: &BiomeRegistry,
    ) -> LoadResult<FlatLevelGeneratorPreset> {
        let fields = Fields::new(value)?;
        let display_item = fields.required("display", |v| {
            let id = parse_identifier(v)?;
            let item = Item::by_name(&id.to_string())
                .ok_or_else(|| LoadError::new(format!("Unknown registry key in minecraft:item: {id}")))?;
            if item == Item::AIR {
                return Err(LoadError::new("Item must not be minecraft:air"));
            }
            Ok(item)
        })?;
        let settings = fields.required("settings", |v| FlatLevelGeneratorSettings::from_json(v, registries, biomes))?;
        Ok(FlatLevelGeneratorPreset { display_item, settings })
    }

    /// The registered preset `id` (memoized, shared like a `Holder.Reference`).
    pub fn load(
        registries: &WorldgenRegistries,
        biomes: &BiomeRegistry,
        id: &Identifier,
    ) -> LoadResult<Arc<FlatLevelGeneratorPreset>> {
        registries.memoized(FLAT_LEVEL_GENERATOR_PRESET_REGISTRY, id, |json, registries| {
            FlatLevelGeneratorPreset::from_json(json, registries, biomes)
        })
    }

    /// Every registered preset in id order.
    pub fn load_all(
        registries: &WorldgenRegistries,
        biomes: &BiomeRegistry,
    ) -> LoadResult<Vec<(Identifier, Arc<FlatLevelGeneratorPreset>)>> {
        registries
            .list(FLAT_LEVEL_GENERATOR_PRESET_REGISTRY)
            .into_iter()
            .map(|id| Self::load(registries, biomes, &id).map(|preset| (id, preset)))
            .collect()
    }
}
