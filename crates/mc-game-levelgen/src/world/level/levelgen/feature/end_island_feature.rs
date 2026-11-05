//! `EndIslandFeature` (`end_island`).

use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature, set_block};
use mc_game_core::world::level::levelgen::GenEnv;
use mc_game_core::world::level::levelgen::WorldGenLevel;
use mc_game_core::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use std::sync::Arc;

pub struct EndIslandFeature;

impl EndIslandFeature {
    pub fn decode(_fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(EndIslandFeature))
    }
}

impl Feature for EndIslandFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let mut size = random.next_int_bounded(3) as f32 + 4.0;
        let mut y = 0;
        while size > 0.5 {
            for x in mth::floor_f32(-size)..=mth::ceil_f32(size) {
                for z in mth::floor_f32(-size)..=mth::ceil_f32(size) {
                    if ((x * x + z * z) as f32) <= (size + 1.0) * (size + 1.0) {
                        set_block(level, origin.offset(x, y, z), Block::END_STONE.default_state());
                    }
                }
            }
            size -= random.next_int_bounded(2) as f32 + 0.5;
            y -= 1;
        }
        true
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static END_ISLAND: TypeEntry<dyn Feature> = TypeEntry { name: "end_island", decode: EndIslandFeature::decode };
