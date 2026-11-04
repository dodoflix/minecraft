//! `net.minecraft.world.level.block.PitcherCropBlock` (extends `DoublePlantBlock`).
use super::crop_block;
use super::double_plant_block;
use super::prelude::*;
use super::vegetation_block::MayPlaceOn;
use mc_data::enums::DoubleBlockHalf;

pub struct PitcherCropBlock;

fn is_lower(state: BlockState) -> bool {
    state.is_block(Block::PITCHER_CROP) && state.get(props::DOUBLE_BLOCK_HALF) == DoubleBlockHalf::Lower
}

fn is_double(age: i32) -> bool {
    age >= 3
}

impl MayPlaceOn for PitcherCropBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_crops")
    }
}

impl BlockBehaviour for PitcherCropBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        if is_lower(state) && !crop_block::has_sufficient_light(level, pos) {
            false
        } else {
            double_plant_block::can_survive(self, state, level, pos)
        }
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if is_double(state.get(props::AGE_4)) {
            double_plant_block::update_shape(state, &*level, pos, direction, neighbour_state)
        } else if state.can_survive(&*level, pos) {
            state
        } else {
            air()
        }
    });
}

register_block_class!("PitcherCropBlock", |_| Box::new(PitcherCropBlock));
