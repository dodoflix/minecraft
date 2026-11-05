//! `net.minecraft.world.level.block.SeagrassBlock` (extends `VegetationBlock`).
use super::prelude::*;
use super::vegetation_block::{self, MayPlaceOn};
use crate::server::level::ServerLevel;
use crate::world::item::BlockPlaceContext;
use crate::world::level::block::bonemeal::BonemealSource;
use mc_data::enums::DoubleBlockHalf;

pub struct SeagrassBlock;

impl MayPlaceOn for SeagrassBlock {
    fn may_place_on(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        state.is_face_sturdy_at(level, pos, Direction::Up) && !state.is_tag("cannot_support_seagrass")
    }
}

impl BlockBehaviour for SeagrassBlock {
    fn is_liquid_block_container(&self) -> bool {
        true
    }
    fn can_place_liquid(&self, _user: Option<&dyn crate::world::entity::Entity>, _level: &dyn BlockGetter, _pos: BlockPos, _state: BlockState, _fluid: mc_data::fluid::Fluid) -> bool {
        false
    }
    fn place_liquid(&self, _level: &mut dyn LevelAccessor, _pos: BlockPos, _state: BlockState, _fluid_state: mc_data::fluid::FluidState) -> bool {
        false
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let fluid_state = level.get_fluid_state(context.get_clicked_pos());
        if fluid_state.is_tag("water") && fluid_state.is_full() { Some(block.default_state()) } else { None }
    }
    fn is_bonemealable(&self) -> bool {
        true
    }
    fn is_valid_bonemeal_target(&self, level: &dyn LevelReader, pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        level.get_block_state(pos.above()).is_block(Block::WATER)
    }
    fn is_bonemeal_success(&self, _level: &mut ServerLevel, _pos: BlockPos, _state: BlockState, _source: BonemealSource) -> bool {
        true
    }
    fn perform_bonemeal(&self, level: &mut ServerLevel, pos: BlockPos, _state: BlockState, _source: BonemealSource) {
        let lower_state = Block::TALL_SEAGRASS.default_state();
        let upper_state = lower_state.set(props::DOUBLE_BLOCK_HALF, DoubleBlockHalf::Upper);
        level.set_block_flags(pos, lower_state, 2);
        level.set_block_flags(pos.above(), upper_state, 2);
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        vegetation_block::can_survive(self, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        let result = vegetation_block::update_shape(state, &*level, pos);
        if !result.is_air() {
            helpers::schedule_water_tick(level, pos);
        }
        result
    });
}

register_block_class!("SeagrassBlock", |_| Box::new(SeagrassBlock));
