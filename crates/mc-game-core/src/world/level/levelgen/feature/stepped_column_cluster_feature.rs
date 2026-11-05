//! `SteppedColumnClusterFeature`: clusters of stepped columns (basalt columns).

use super::{Feature, set_block};
use crate::world::level::levelgen::blockpredicates::{self, BlockPredicate};
use crate::world::level::levelgen::codec::{BlockSet, TypeEntry};
use crate::world::level::levelgen::env::GenEnv;
use crate::world::level::levelgen::holder::Holder;
use crate::world::level::levelgen::stateproviders::BlockStateProvider;
use crate::world::level::levelgen::world_gen_level::WorldGenLevel;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_core::random::RandomSource;
use mc_worldgen::provider::IntProvider;
use std::sync::Arc;

pub struct SteppedColumnClusterFeature {
    pub block: Holder<dyn BlockStateProvider>,
    pub continue_through: Arc<dyn BlockPredicate>,
    pub can_replace: Arc<dyn BlockPredicate>,
    pub cannot_place_on: BlockSet,
    pub cluster_reach: IntProvider,
    pub column_count: IntProvider,
    pub column_reach: IntProvider,
    pub height: IntProvider,
}

impl Feature for SteppedColumnClusterFeature {
    fn place(&self, level: &mut dyn WorldGenLevel, generator: &GenEnv, random: &mut dyn RandomSource, origin: BlockPos) -> bool {
        let lava_sea_level = generator.get_sea_level();
        if !self.can_place_at(level, origin) {
            return false;
        }
        let column_height = self.height.sample(random);
        let cluster_reach = column_height.min(self.cluster_reach.sample(random));
        let count = self.column_count.sample(random);
        let mut placed = false;

        // `BlockPos.randomBetweenClosed`: lazy, each position draws x, y, z right before its body runs.
        for _ in 0..count {
            let x = origin.x() - cluster_reach + random.next_int_bounded(2 * cluster_reach + 1);
            let y = origin.y() + random.next_int_bounded(1);
            let z = origin.z() - cluster_reach + random.next_int_bounded(2 * cluster_reach + 1);
            let pos = BlockPos::new(x, y, z);
            let blocks_to_place_y = column_height - pos.dist_manhattan(&origin);
            if blocks_to_place_y >= 0 {
                let reach = self.column_reach.sample(random);
                placed |= self.place_column(level, random, lava_sea_level, pos, blocks_to_place_y, reach);
            }
        }
        placed
    }
}

impl SteppedColumnClusterFeature {
    fn place_column(&self, level: &mut dyn WorldGenLevel, random: &mut dyn RandomSource, _lava_sea_level: i32, origin: BlockPos, column_height: i32, reach: i32) -> bool {
        let mut placed_any = false;

        for pos in BlockPos::between_closed_xyz(origin.x() - reach, origin.y(), origin.z() - reach, origin.x() + reach, origin.y(), origin.z() + reach) {
            let step_limit = pos.dist_manhattan(&origin);
            let column_pos = if self.can_replace.test(&*level, pos) {
                self.find_surface(level, pos, step_limit)
            } else {
                self.find_air(level, pos, step_limit)
            };
            if let Some(column_pos) = column_pos {
                let mut blocks_y = column_height - step_limit / 2;
                let mut cursor = column_pos;
                while blocks_y >= 0 {
                    if self.can_replace.test(&*level, cursor) {
                        let state = self.block.value().get_state(&*level, random, cursor);
                        set_block(level, cursor, state);
                        cursor = cursor.relative(Direction::Up);
                        placed_any = true;
                    } else {
                        if !self.continue_through.test(&*level, cursor) {
                            break;
                        }
                        cursor = cursor.relative(Direction::Up);
                    }
                    blocks_y -= 1;
                }
            }
        }
        placed_any
    }

    fn find_surface(&self, level: &dyn WorldGenLevel, mut cursor: BlockPos, mut limit: i32) -> Option<BlockPos> {
        while cursor.y() > level.get_min_y() + 1 && limit > 0 {
            limit -= 1;
            if self.can_place_at(level, cursor) {
                return Some(cursor);
            }
            cursor = cursor.relative(Direction::Down);
        }
        None
    }

    fn can_place_at(&self, level: &dyn WorldGenLevel, cursor: BlockPos) -> bool {
        if !self.can_replace.test(level, cursor) {
            return false;
        }
        let block_state = level.get_block_state(cursor.below());
        !block_state.is_air() && !self.cannot_place_on.contains(block_state.block())
    }

    fn find_air(&self, level: &dyn WorldGenLevel, mut cursor: BlockPos, mut limit: i32) -> Option<BlockPos> {
        while cursor.y() <= level.get_max_y() && limit > 0 {
            limit -= 1;
            let block_state = level.get_block_state(cursor);
            if self.cannot_place_on.contains(block_state.block()) {
                return None;
            }
            if block_state.is_air() {
                return Some(cursor);
            }
            cursor = cursor.relative(Direction::Up);
        }
        None
    }
}

#[linkme::distributed_slice(super::FEATURE_TYPES)]
static STEPPED_COLUMN_CLUSTER: TypeEntry<dyn Feature> = TypeEntry {
    name: "stepped_column_cluster",
    decode: |fields, ctx| {
        Ok(Arc::new(SteppedColumnClusterFeature {
            block: fields.required("block", |v| ctx.block_state_provider(v))?,
            continue_through: fields.required("continue_through", |v| blockpredicates::decode(v, ctx))?,
            can_replace: fields.required("can_replace", |v| blockpredicates::decode(v, ctx))?,
            cannot_place_on: fields.required("cannot_place_on", BlockSet::from_json)?,
            cluster_reach: fields.required("cluster_reach", |v| IntProvider::from_json_range(v, 0, 13))?,
            column_count: fields.required("column_count", |v| IntProvider::from_json_range(v, 1, 150))?,
            column_reach: fields.required("column_reach", |v| IntProvider::from_json_range(v, 0, 3))?,
            height: fields.required("height", |v| IntProvider::from_json_range(v, 1, 10))?,
        }))
    },
};
