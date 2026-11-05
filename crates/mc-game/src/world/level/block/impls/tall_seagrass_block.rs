//! `net.minecraft.world.level.block.TallSeagrassBlock` (extends `DoublePlantBlock`).
use super::double_plant_block;
use super::prelude::*;
use super::vegetation_block::MayPlaceOn;
use crate::server::level::ServerLevel;
use crate::world::item::{BlockPlaceContext, ItemStack};
use mc_data::enums::DoubleBlockHalf;

pub struct TallSeagrassBlock;

impl MayPlaceOn for TallSeagrassBlock {
    fn may_place_on(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        state.is_face_sturdy_at(level, pos, Direction::Up) && !state.is_tag("cannot_support_seagrass")
    }
}

impl BlockBehaviour for TallSeagrassBlock {
    fn is_liquid_block_container(&self) -> bool {
        true
    }
    fn can_place_liquid(&self, _user: Option<&dyn crate::world::entity::Entity>, _level: &dyn BlockGetter, _pos: BlockPos, _state: BlockState, _fluid: mc_data::fluid::Fluid) -> bool {
        false
    }
    fn place_liquid(&self, _level: &mut dyn LevelAccessor, _pos: BlockPos, _state: BlockState, _fluid_state: mc_data::fluid::FluidState) -> bool {
        false
    }
    fn get_clone_item_stack(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> ItemStack {
        ItemStack::new(mc_data::Item::SEAGRASS)
    }
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        // `DoublePlantBlock.getStateForPlacement`
        let pos = context.get_clicked_pos();
        if pos.y() >= level.get_max_y() || !level.get_block_state(pos.above()).can_be_replaced_by(context) {
            return None;
        }
        let state = block.default_state();
        let fluid_state = level.get_fluid_state(context.get_clicked_pos().above());
        if fluid_state.is_tag("water") && fluid_state.is_full() { Some(state) } else { None }
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        if state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Upper {
            let below_state = level.get_block_state(pos.below());
            below_state.is_block(state.block()) && below_state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower
        } else {
            let fluid_state = level.get_fluid_state(pos);
            double_plant_block::can_survive(self, state, level, pos) && fluid_state.is_tag("water") && fluid_state.is_full()
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        double_plant_block::update_shape(state, &*level, pos, direction, neighbour_state)
    });
}

register_block_class!("TallSeagrassBlock", |_| Box::new(TallSeagrassBlock));
