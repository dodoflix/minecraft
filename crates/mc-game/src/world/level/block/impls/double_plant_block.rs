//! `net.minecraft.world.level.block.DoublePlantBlock` (extends `VegetationBlock`; also `TallFlowerBlock`).
use super::prelude::*;
use super::vegetation_block::{self, MayPlaceOn};
use mc_data::enums::DoubleBlockHalf;

pub struct DoublePlantBlock;
impl MayPlaceOn for DoublePlantBlock {}

/// `DoublePlantBlock.updateShape` (`neighbour_state.is(this)` compares with the state's own block).
pub fn update_shape(state: BlockState, level: &dyn LevelReader, pos: BlockPos, direction_to_neighbour: Direction, neighbour_state: BlockState) -> BlockState {
    let half = state.get(props::DOUBLE_BLOCK_HALF);
    if direction_to_neighbour.get_axis() != Axis::Y
        || (half == DoubleBlockHalf::Lower) != (direction_to_neighbour == Direction::Up)
        || (neighbour_state.is_block(state.block()) && neighbour_state.get(props::DOUBLE_BLOCK_HALF) != half)
    {
        if half == DoubleBlockHalf::Lower && direction_to_neighbour == Direction::Down && !state.can_survive(level, pos) {
            air()
        } else {
            vegetation_block::update_shape(state, level, pos)
        }
    } else {
        air()
    }
}

/// `DoublePlantBlock.canSurvive` (`this` supplies `mayPlaceOn` for the lower half).
pub fn can_survive(this: &dyn MayPlaceOn, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
    if state.get(props::DOUBLE_BLOCK_HALF) != DoubleBlockHalf::Upper {
        vegetation_block::can_survive(this, level, pos)
    } else {
        let below_state = level.get_block_state(pos.below());
        below_state.is_block(state.block()) && below_state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower
    }
}

/// `DoublePlantBlock.placeAt`.
pub fn place_at(level: &mut dyn LevelAccessor, state: BlockState, lower_pos: BlockPos, update_type: u32) {
    let upper_pos = lower_pos.above();
    let lower = copy_waterlogged_from(&*level, lower_pos, state.set(props::DOUBLE_BLOCK_HALF, DoubleBlockHalf::Lower));
    level.set_block_flags(lower_pos, lower, update_type);
    let upper = copy_waterlogged_from(&*level, upper_pos, state.set(props::DOUBLE_BLOCK_HALF, DoubleBlockHalf::Upper));
    level.set_block_flags(upper_pos, upper, update_type);
}

/// `DoublePlantBlock.copyWaterloggedFrom`.
pub fn copy_waterlogged_from(level: &dyn LevelReader, pos: BlockPos, state: BlockState) -> BlockState {
    if state.has_property(props::WATERLOGGED) { state.set(props::WATERLOGGED, helpers::is_water_at(level, pos)) } else { state }
}

impl BlockBehaviour for DoublePlantBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        can_survive(self, state, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        update_shape(state, &*level, pos, direction, neighbour_state)
    });
}

register_block_class!("DoublePlantBlock", |_| Box::new(DoublePlantBlock));
