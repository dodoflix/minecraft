//! `VillagerType`: the biome -> villager type table.

use mc_data::VillagerType;

/// `VillagerType.byBiome(biome)` for the biome's registry key (`None` for an unregistered biome).
pub fn by_biome(biome: Option<&str>) -> VillagerType {
    let name = biome.map(|b| b.strip_prefix("minecraft:").unwrap_or(b));
    match name {
        Some("badlands" | "desert" | "eroded_badlands" | "wooded_badlands") => VillagerType::DESERT,
        Some("bamboo_jungle" | "jungle" | "sparse_jungle") => VillagerType::JUNGLE,
        Some("savanna_plateau" | "savanna" | "windswept_savanna") => VillagerType::SAVANNA,
        Some(
            "deep_frozen_ocean" | "frozen_ocean" | "frozen_river" | "ice_spikes" | "snowy_beach"
            | "snowy_taiga" | "snowy_plains" | "grove" | "snowy_slopes" | "frozen_peaks"
            | "jagged_peaks",
        ) => VillagerType::SNOW,
        Some("swamp" | "mangrove_swamp") => VillagerType::SWAMP,
        Some(
            "old_growth_spruce_taiga"
            | "old_growth_pine_taiga"
            | "windswept_gravelly_hills"
            | "windswept_hills"
            | "taiga"
            | "windswept_forest",
        ) => VillagerType::TAIGA,
        _ => DEFAULT_TYPE,
    }
}

use super::villager_data::DEFAULT_TYPE;
