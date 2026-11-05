//! `BlockPileFeature`.

use super::{FEATURE_TYPES, Feature};
use crate::world::level::block::BlockStateExt;
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::stateproviders::BlockStateProvider;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use crate::world::level::{LevelAccessor};
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::Block;
use std::sync::Arc;

pub struct BlockPileFeature {
    pub state_provider: Holder<dyn BlockStateProvider>,
}

impl BlockPileFeature {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(BlockPileFeature { state_provider: fields.required("state_provider", |v| ctx.block_state_provider(v))? }))
    }

    fn may_place_on(level: &dyn LevelAccessor, block_pos: BlockPos, random: &mut dyn RandomSource) -> bool {
        let below = block_pos.below();
        let below_state = level.get_block_state(below);
        if below_state.is_block(Block::DIRT_PATH) { random.next_boolean() } else { below_state.is_face_sturdy_at(level, below, Direction::Up) }
    }

    fn try_place_block(&self, level: &mut dyn WorldGenLevel, block_pos: BlockPos, random: &mut dyn RandomSource) {
        if level.is_empty_block(block_pos) && Self::may_place_on(&*level, block_pos, random) {
            let state = self.state_provider.value().get_state(&*level, random, block_pos);
            level.set_block_flags(block_pos, state, 260);
        }
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static BLOCK_PILE: TypeEntry<dyn Feature> = TypeEntry { name: "block_pile", decode: BlockPileFeature::decode };

impl Feature for BlockPileFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        if origin.y() < level.get_min_y() + 5 {
            return false;
        }
        let xr = 2 + random.next_int_bounded(2);
        let zr = 2 + random.next_int_bounded(2);

        for block_pos in BlockPos::between_closed(&origin.offset(-xr, 0, -zr), &origin.offset(xr, 1, zr)) {
            let xd = origin.x() - block_pos.x();
            let zd = origin.z() - block_pos.z();
            let a = random.next_float();
            let b = random.next_float();
            if (xd * xd + zd * zd) as f32 <= a * 10.0 - b * 6.0 {
                self.try_place_block(level, block_pos, random);
            // the literal is a double: `float < 0.031`
            } else if f64::from(random.next_float()) < 0.031 {
                self.try_place_block(level, block_pos, random);
            }
        }
        true
    }
}
