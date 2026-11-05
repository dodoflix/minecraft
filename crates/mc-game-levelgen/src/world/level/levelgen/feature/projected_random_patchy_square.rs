//! `ProjectedRandomPatchySquare`.

use mc_game_core::world::level::levelgen::feature::{FEATURE_TYPES, Feature};
use mc_game_core::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use mc_game_core::world::level::levelgen::codec::{Fields, LoadError, LoadResult, TypeEntry, parse_int};
use mc_game_core::world::level::levelgen::env::GenEnv;
use mc_game_core::world::level::levelgen::holder::Holder;
use mc_game_core::world::level::levelgen::registries::DecodeCtx;
use mc_game_core::world::level::levelgen::stateproviders::BlockStateProvider;
use mc_game_core::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::direction::Direction;
use mc_core::math::mth;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct ProjectedRandomPatchySquare {
    pub block: Holder<dyn BlockStateProvider>,
    pub project_through: Arc<dyn BlockPredicate>,
    pub size: IntProvider,
    pub max_projection_height: i32,
}

impl ProjectedRandomPatchySquare {
    fn decode(fields: &Fields<'_>, ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(ProjectedRandomPatchySquare {
            block: fields.required("block", |v| ctx.block_state_provider(v))?,
            project_through: fields.required("project_through", |v| blockpredicates::decode(v, ctx))?,
            size: fields.required("size", |v| IntProvider::from_json_range(v, 1, 16))?,
            max_projection_height: fields.required("max_projection_height", |v| {
                let n = parse_int(v)?;
                if n >= 0 { Ok(n) } else { Err(LoadError::new(format!("Value must be non-negative: {n}"))) }
            })?,
        }))
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static PROJECTED_RANDOM_PATCHY_SQUARE: TypeEntry<dyn Feature> =
    TypeEntry { name: "projected_random_patchy_square", decode: ProjectedRandomPatchySquare::decode };

impl Feature for ProjectedRandomPatchySquare {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, pos: BlockPos) -> bool {
        let size = self.size.sample(random);
        let bound = mth::square_i32(size) + 1;

        for dx in -size..=size {
            for dz in -size..=size {
                let probability = mth::abs_i32(dx) * mth::abs_i32(dz);
                if random.next_int_bounded(bound) < bound - probability {
                    let mut base_pos = pos.offset(dx, 0, dz);
                    let mut drop = self.max_projection_height;

                    while self.project_through.test(&*level, base_pos.relative(Direction::Down)) {
                        base_pos = base_pos.relative(Direction::Down);
                        drop -= 1;
                        if drop <= 0 {
                            break;
                        }
                    }

                    if let Some(state) = self.block.value().get_optional_state(&*level, random, base_pos) {
                        level.set_block_flags(base_pos, state, 2);
                    }
                }
            }
        }
        true
    }
}
