//! `net.minecraft.world.level.block.MangrovePropaguleBlock` (extends `SaplingBlock`).
use super::prelude::*;
use super::sapling_block;
use super::vegetation_block::{self, MayPlaceOn};
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;
use crate::world::level::block::bonemeal::BonemealSource;
use crate::world::level::block::grower::TreeGrower;
use mc_data::fluid::Fluid;

pub struct MangrovePropaguleBlock;

impl MayPlaceOn for MangrovePropaguleBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_mangrove_propagule")
    }
}

/// `MangrovePropaguleBlock.createNewHangingPropagule(age)`.
pub fn create_new_hanging_propagule(age: i32) -> BlockState {
    Block::MANGROVE_PROPAGULE.default_state().set(props::HANGING, true).set(props::AGE_4, age)
}

fn is_fully_grown(state: BlockState) -> bool {
    state.get(props::AGE_4) == 4
}

impl BlockBehaviour for MangrovePropaguleBlock {
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let is_water_source = level.get_fluid_state(context.get_clicked_pos()).fluid() == Fluid::WATER;
        Some(block.default_state().set(props::WATERLOGGED, is_water_source).set(props::AGE_4, 4))
    }
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if !state.get(props::HANGING) {
            if level.random().next_int_bounded(7) == 0 {
                sapling_block::advance_tree(&TreeGrower::MANGROVE, level, pos, state);
            }
        } else if !is_fully_grown(state) {
            level.set_block_flags(pos, state.cycle(props::AGE_4), 2);
        }
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, _level: &dyn LevelReader, _pos: BlockPos, state: BlockState, _source: BonemealSource) -> bool {
        !state.get(props::HANGING) || !is_fully_grown(state)
    }
    fn is_bonemeal_success(&self, level: &mut ServerLevel, _pos: BlockPos, state: BlockState, _source: BonemealSource) -> bool {
        if state.get(props::HANGING) { !is_fully_grown(state) } else { level.random().next_float() < 0.45 }
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _source: BonemealSource) {
        if state.get(props::HANGING) && !is_fully_grown(state) {
            level.set_block_flags(pos, state.cycle(props::AGE_4), 2);
        } else {
            sapling_block::advance_tree(&TreeGrower::MANGROVE, level, pos, state);
        }
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        if state.get(props::HANGING) {
            level.get_block_state(pos.above()).is_tag("supports_hanging_mangrove_propagule")
        } else {
            vegetation_block::can_survive(self, level, pos)
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        if direction == Direction::Up && !state.can_survive(&*level, pos) { air() } else { vegetation_block::update_shape(state, &*level, pos) }
    });
}

register_block_class!("MangrovePropaguleBlock", |_| Box::new(MangrovePropaguleBlock));
