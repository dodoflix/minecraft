//! `DeltaFeature`.

use super::{FEATURE_TYPES, Feature};
use crate::world::level::block::BlockStateExt;
use crate::world::level::levelgen::codec::{Fields, LoadResult, TypeEntry, parse_block_state};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::registries::DecodeCtx;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_data::block::{Block, BlockState};
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

const CANNOT_REPLACE: [Block; 7] =
    [Block::BEDROCK, Block::NETHER_BRICKS, Block::NETHER_BRICK_FENCE, Block::NETHER_BRICK_STAIRS, Block::NETHER_WART, Block::CHEST, Block::SPAWNER];
const RIM_SPAWN_CHANCE: f64 = 0.9;

pub struct DeltaFeature {
    pub contents: BlockState,
    pub rim: BlockState,
    pub size: IntProvider,
    pub rim_size: IntProvider,
}

impl DeltaFeature {
    fn decode(fields: &Fields<'_>, _ctx: &DecodeCtx<'_>) -> LoadResult<Arc<dyn Feature>> {
        Ok(Arc::new(DeltaFeature {
            contents: fields.required("contents", parse_block_state)?,
            rim: fields.required("rim", parse_block_state)?,
            size: fields.required("size", |v| IntProvider::from_json_range(v, 0, 16))?,
            rim_size: fields.required("rim_size", |v| IntProvider::from_json_range(v, 0, 16))?,
        }))
    }

    fn is_clear(&self, level: &dyn LevelAccessor, pos: BlockPos) -> bool {
        let state = level.get_block_state(pos);
        if state.is_block(self.contents.block()) {
            return false;
        }
        if CANNOT_REPLACE.contains(&state.block()) {
            return false;
        }
        for d in Direction::ALL {
            let is_air = level.get_block_state(pos.relative(d)).is_air();
            if is_air && d != Direction::Up || !is_air && d == Direction::Up {
                return false;
            }
        }
        true
    }
}

#[linkme::distributed_slice(FEATURE_TYPES)]
static DELTA_FEATURE: TypeEntry<dyn Feature> = TypeEntry { name: "delta_feature", decode: DeltaFeature::decode };

impl Feature for DeltaFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, _generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let mut any_placed = false;
        let spawn_rim = random.next_double() < RIM_SPAWN_CHANCE;
        let rim_x = if spawn_rim { self.rim_size.sample(random) } else { 0 };
        let rim_z = if spawn_rim { self.rim_size.sample(random) } else { 0 };
        let has_rim = spawn_rim && rim_x != 0 && rim_z != 0;
        let radius_x = self.size.sample(random);
        let radius_z = self.size.sample(random);
        let radius_limit = radius_x.max(radius_z);

        for pos in BlockPos::within_box_by_manhattan_distance(&origin, radius_x, 0, radius_z) {
            if pos.dist_manhattan(&origin) > radius_limit {
                break;
            }

            if self.is_clear(&*level, pos) {
                if has_rim {
                    any_placed = true;
                    level.set_block_and_update(pos, self.rim);
                }

                let pos_offset = pos.offset(rim_x, 0, rim_z);
                if self.is_clear(&*level, pos_offset) {
                    any_placed = true;
                    level.set_block_and_update(pos_offset, self.contents);
                }
            }
        }
        any_placed
    }
}
