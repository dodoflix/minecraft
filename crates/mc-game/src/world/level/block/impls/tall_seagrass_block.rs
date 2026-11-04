//! `net.minecraft.world.level.block.TallSeagrassBlock` (extends `DoublePlantBlock`).
use super::double_plant_block;
use super::prelude::*;
use super::vegetation_block::MayPlaceOn;
use mc_data::enums::DoubleBlockHalf;

pub struct TallSeagrassBlock;

impl MayPlaceOn for TallSeagrassBlock {
    fn may_place_on(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        state.is_face_sturdy_at(level, pos, Direction::Up) && !state.is_tag("cannot_support_seagrass")
    }
}

impl BlockBehaviour for TallSeagrassBlock {
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
