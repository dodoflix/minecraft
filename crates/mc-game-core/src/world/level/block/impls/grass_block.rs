//! `net.minecraft.world.level.block.GrassBlock`: spreading snowy block + bone meal (grass / flower patches around).
use super::prelude::*;
use super::{snowy_block, spreading_snowy_block};
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;
use crate::world::level::block::bonemeal::{BonemealSource, BonemealType};

pub struct GrassBlock;

/// `GrassBlock.stopBonemealSpread`.
fn stop_bonemeal_spread(level: &ServerLevel, this: Block, test_pos: BlockPos) -> bool {
    !level.get_block_state(test_pos.below()).is_block(this) || level.get_block_state(test_pos).is_collision_shape_full_block_at(level, test_pos)
}

/// `GrassBlock.placeBonemealEffect`.
fn place_bonemeal_effect(level: &mut ServerLevel, test_pos: BlockPos, source: BonemealSource) {
    let test_state = level.get_block_state(test_pos);
    if test_state.is_block(Block::SHORT_GRASS) && level.random().next_float() < 0.1 {
        let grass = crate::world::level::block::behaviour(Block::SHORT_GRASS);
        if grass.is_valid_bonemeal_target(&*level, test_pos, test_state, source) {
            grass.perform_bonemeal(level, test_pos, test_state, source);
        }
    }
    if test_state.is_air() && !level.is_outside_build_height(test_pos.y()) {
        if level.random().next_float() < 0.125 {
            let features = level.get_bone_meal_features(test_pos);
            if features.is_empty() {
                return;
            }
            let index = level.random().next_int_bounded(features.len() as i32) as usize;
            level.place_feature_value(features[index].as_ref(), test_pos);
        } else {
            level.place_placed_feature("minecraft:grass_bonemeal", test_pos);
        }
    }
}

impl BlockBehaviour for GrassBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| { snowy_block::update_shape(state, direction, neighbour_state) });
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        spreading_snowy_block::random_tick(state, level, pos);
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        snowy_block::get_state_for_placement(block, level, context)
    }

    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, level: &dyn LevelReader, pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        level.get_block_state(pos.above()).is_air() && !level.is_outside_build_height(pos.above().y())
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, source: BonemealSource) {
        let above = pos.above();
        'attempts: for attempt in 0..128 {
            let mut test_pos = above;
            let randomize_count = attempt / 16;
            for _ in 0..randomize_count {
                let dx = level.random().next_int_between_inclusive(-1, 1);
                let dy = level.random().next_int_between_inclusive(-1, 1) * level.random().next_int_bounded(3) / 2;
                let dz = level.random().next_int_between_inclusive(-1, 1);
                test_pos = test_pos.offset(dx, dy, dz);
                if stop_bonemeal_spread(level, state.block(), test_pos) {
                    continue 'attempts;
                }
            }
            place_bonemeal_effect(level, test_pos, source);
        }
    }
    fn get_bonemeal_type(&self) -> BonemealType {
        BonemealType::NeighborSpreader
    }
}

register_block_class!("GrassBlock", |_| Box::new(GrassBlock));
