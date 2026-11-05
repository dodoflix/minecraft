//! `EndPlatformFeature` (`end_platform`): the 5x5 obsidian platform at the End spawn.

use super::{FEATURE_TYPES, Feature};
use crate::world::level::block::BlockStateExt;
use crate::world::level::levelgen::GenEnv;
use crate::world::level::levelgen::WorldGenLevel;
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::LevelAccessor;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use std::sync::Arc;

pub struct EndPlatformFeature;

impl EndPlatformFeature {
    pub fn decode(_fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(EndPlatformFeature))
    }

    /// `createEndPlatform(level, origin, dropResources)`.
    pub fn create_end_platform(level: &mut dyn LevelAccessor, origin: BlockPos, drop_resources: bool) {
        for dz in -2..=2 {
            for dx in -2..=2 {
                for dy in -1..3 {
                    let block_pos = origin.offset(dx, dy, dz);
                    let block = if dy == -1 { Block::OBSIDIAN } else { Block::AIR };
                    if !level.get_block_state(block_pos).is_block(block) {
                        if drop_resources {
                            level.destroy_block(block_pos, true, None, 512);
                        }
                        level.set_block_and_update(block_pos, block.default_state());
                    }
                }
            }
        }
    }
}

impl Feature for EndPlatformFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, _random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        Self::create_end_platform(level, origin, false);
        true
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static END_PLATFORM: TypeEntry<dyn Feature> = TypeEntry { name: "end_platform", decode: EndPlatformFeature::decode };
