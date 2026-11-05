//! `net.minecraft.world.level.block.SeaPickleBlock` (extends `VegetationBlock`).
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::MayPlaceOn;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::bonemeal::BonemealSource;
use mc_data::fluid::Fluid;

pub struct SeaPickleBlock;

impl MayPlaceOn for SeaPickleBlock {
    fn may_place_on(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        !state.get_collision_shape(level, pos).get_face_shape(Direction::Up).is_empty() || state.is_face_sturdy_at(level, pos, Direction::Up)
    }
}

/// `SeaPickleBlock.isDead`.
pub fn is_dead(state: BlockState) -> bool {
    !state.get(props::WATERLOGGED)
}

impl BlockBehaviour for SeaPickleBlock {
    mc_game_core::simple_waterlogged!();
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let state = level.get_block_state(context.get_clicked_pos());
        if state.is_block(block) {
            Some(state.set(props::PICKLES, (state.get(props::PICKLES) + 1).min(4)))
        } else {
            let replaced_fluid_state = level.get_fluid_state(context.get_clicked_pos());
            let is_water_source = replaced_fluid_state.get_type() == Fluid::WATER;
            Some(block.default_state().set(props::WATERLOGGED, is_water_source))
        }
    }
    fn can_be_replaced(&self, state: BlockState, context: &BlockPlaceContext) -> bool {
        let held = context.use_on.get_item_in_hand();
        if !context.is_secondary_use_active() && !held.is_empty() && Some(held.item()) == state.block().item() && state.get(props::PICKLES) < 4 {
            true
        } else {
            state.can_be_replaced() && (held.is_empty() || Some(held.item()) != state.block().item())
        }
    }
    fn is_pathfindable(&self, _state: BlockState, _ty: mc_data::block::PathComputationType) -> bool {
        false
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, level: &dyn LevelReader, pos: BlockPos, state: BlockState, _source: BonemealSource) -> bool {
        !is_dead(state) && level.get_block_state(pos.below()).is_tag("coral_blocks")
    }
    fn is_bonemeal_success(&self, _level: &mut ServerLevel, _pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        true
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _source: BonemealSource) {
        let mut z_span = 1;
        let mut count = 0;
        let x_start = pos.x() - 2;
        let mut z_off_set = 0;
        for x in 0..5 {
            for z in 0..z_span {
                let end_y = 2 + pos.y() - 1;
                for start_y in (end_y - 2)..end_y {
                    let position = BlockPos::new(x_start + x, start_y, pos.z() - z_off_set + z);
                    if position != pos && level.random().next_int_bounded(6) == 0 && level.get_block_state(position).is_block(Block::WATER) {
                        let below_state = level.get_block_state(position.below());
                        if below_state.is_tag("coral_blocks") {
                            let pickles = level.random().next_int_bounded(4) + 1;
                            level.set_block_and_update(position, Block::SEA_PICKLE.default_state().set(props::PICKLES, pickles));
                        }
                    }
                }
            }
            if count < 2 {
                z_span += 2;
                z_off_set += 1;
            } else {
                z_span -= 2;
                z_off_set -= 1;
            }
            count += 1;
        }
        level.set_block_flags(pos, state.set(props::PICKLES, 4), 2);
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_pos = pos.below();
        self.may_place_on(level.get_block_state(below_pos), level, below_pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !state.can_survive(&*level, pos) {
            air()
        } else {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            state
        }
    });
}

register_block_class!("SeaPickleBlock", |_| Box::new(SeaPickleBlock));
