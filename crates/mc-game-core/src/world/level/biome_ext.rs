//! `Biome` methods that read the level (`shouldFreeze`, `shouldSnow`) and `Level.isWaterAt`.

use super::LevelReader;
use super::block::BlockStateExt;
use super::material::FluidStateExt;
use super::precipitation::Precipitation;
use crate::world::level::LightLayer;
use mc_core::pos::BlockPos;
use mc_data::block::Block;
use mc_data::fluid::Fluid;
use mc_worldgen::biome::Biome;

/// `LevelReader.isWaterAt(pos)`.
pub fn is_water_at(level: &dyn LevelReader, pos: BlockPos) -> bool {
    level.get_fluid_state(pos).is_tag("water")
}

/// `Biome.shouldFreeze(level, pos)` (`checkNeighbors` = true).
pub fn should_freeze(biome: &Biome, level: &dyn LevelReader, pos: BlockPos) -> bool {
    should_freeze_checked(biome, level, pos, true)
}

/// `Biome.shouldFreeze(level, pos, checkNeighbors)`.
pub fn should_freeze_checked(biome: &Biome, level: &dyn LevelReader, pos: BlockPos, check_neighbors: bool) -> bool {
    if biome.warm_enough_to_rain(pos, level.get_sea_level()) {
        return false;
    }
    if !level.is_outside_build_height(pos.y()) && level.get_brightness(LightLayer::Block, pos) < 10 {
        let block_state = level.get_block_state(pos);
        let fluid_state = level.get_fluid_state(pos);
        if fluid_state.fluid() == Fluid::WATER && block_state.block().data().classes.contains(&"LiquidBlock") {
            if !check_neighbors {
                return true;
            }
            let surrounded_by_water = is_water_at(level, pos.west()) && is_water_at(level, pos.east()) && is_water_at(level, pos.north()) && is_water_at(level, pos.south());
            if !surrounded_by_water {
                return true;
            }
        }
    }
    false
}

/// `Biome.shouldSnow(level, pos)`.
pub fn should_snow(biome: &Biome, level: &dyn LevelReader, pos: BlockPos) -> bool {
    if biome.get_precipitation_at(pos, level.get_sea_level()) != Precipitation::Snow {
        return false;
    }
    if !level.is_outside_build_height(pos.y()) && level.get_brightness(LightLayer::Block, pos) < 10 {
        let state = level.get_block_state(pos);
        if (state.is_air() || state.block() == Block::SNOW) && Block::SNOW.default_state().can_survive(level, pos) {
            return true;
        }
    }
    false
}
