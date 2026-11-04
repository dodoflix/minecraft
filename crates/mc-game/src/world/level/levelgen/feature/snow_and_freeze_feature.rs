//! `SnowAndFreezeFeature` (`freeze_top_layer`).

use super::{FEATURE_TYPES, Feature};
use crate::world::level::biome_ext::{should_freeze_checked, should_snow};
use crate::world::level::levelgen::codec::TypeEntry;
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use crate::world::level::{BlockGetter, LevelAccessor, LevelReader};
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use mc_data::props;
use mc_world::heightmap::HeightmapType;
use std::sync::Arc;

pub struct SnowAndFreezeFeature;

#[linkme::distributed_slice(FEATURE_TYPES)]
static FREEZE_TOP_LAYER: TypeEntry<dyn Feature> = TypeEntry { name: "freeze_top_layer", decode: |_, _| Ok(Arc::new(SnowAndFreezeFeature)) };

impl Feature for SnowAndFreezeFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, _random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        for dx in 0..16 {
            for dz in 0..16 {
                let x = origin.x() + dx;
                let z = origin.z() + dz;
                let y = level.get_height_at(HeightmapType::MotionBlocking, x, z);
                let top_pos = BlockPos::new(x, y, z);
                let below_pos = top_pos.relative(Direction::Down);
                let biome = generator.biomes.get(level.get_biome(top_pos));
                if should_freeze_checked(biome, &*level, below_pos, false) {
                    level.set_block_flags(below_pos, Block::ICE.default_state(), 2);
                }

                if should_snow(biome, &*level, top_pos) {
                    level.set_block_flags(top_pos, Block::SNOW.default_state(), 2);
                    let below_state = level.get_block_state(below_pos);
                    if below_state.has_property(props::SNOWY) {
                        level.set_block_flags(below_pos, below_state.set(props::SNOWY, true), 2);
                    }
                }
            }
        }
        true
    }
}
