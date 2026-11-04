//! `RandomNeighborSpreadFeature`.

use super::{FEATURE_TYPES, Feature};
use crate::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use crate::world::level::levelgen::codec::{BlockSet, Fields, LoadResult, TypeEntry};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::stateproviders::BlockStateProvider;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct RandomNeighborSpreadFeature {
    pub block: Holder<dyn BlockStateProvider>,
    pub accepted_neighbors: BlockSet,
    pub can_replace: Arc<dyn BlockPredicate>,
    pub attempts: IntProvider,
    pub xz_offset: IntProvider,
    pub y_offset: IntProvider,
}

impl RandomNeighborSpreadFeature {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(RandomNeighborSpreadFeature {
            block: fields.required("block", |v| ctx.block_state_provider(v))?,
            accepted_neighbors: fields.required("accepted_neighbors", BlockSet::from_json)?,
            can_replace: fields.required("can_replace", |v| blockpredicates::decode(v, ctx))?,
            attempts: fields.required("attempts", |v| IntProvider::from_json_range(v, 1, 3000))?,
            xz_offset: fields.required("xz_offset", |v| IntProvider::from_json_range(v, -16, 16))?,
            y_offset: fields.required("y_offset", |v| IntProvider::from_json_range(v, -16, 16))?,
        }))
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static RANDOM_NEIGHBOR_SPREAD: TypeEntry<dyn Feature> = TypeEntry { name: "random_neighbor_spread", decode: RandomNeighborSpreadFeature::decode };

impl Feature for RandomNeighborSpreadFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let state = self.block.value().get_state(&*level, random, origin);
        level.set_block_flags(origin, state, 2);
        let attempts = self.attempts.sample(random);

        for _ in 0..attempts {
            let dx = self.xz_offset.sample(random);
            let dy = self.y_offset.sample(random);
            let dz = self.xz_offset.sample(random);
            let place_pos = origin.offset(dx, dy, dz);
            if self.can_replace.test(&*level, place_pos) {
                let mut neighbours = 0;
                for direction in Direction::ALL {
                    if self.accepted_neighbors.contains(level.get_block_state(place_pos.relative(direction)).block()) {
                        neighbours += 1;
                    }
                    if neighbours > 1 {
                        break;
                    }
                }

                if neighbours == 1 {
                    let state = self.block.value().get_state(&*level, random, place_pos);
                    level.set_block_flags(place_pos, state, 2);
                }
            }
        }
        true
    }
}
