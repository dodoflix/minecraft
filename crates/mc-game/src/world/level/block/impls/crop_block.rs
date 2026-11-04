//! `net.minecraft.world.level.block.CropBlock` (extends `VegetationBlock`; also wheat, carrots, potatoes, beetroots,
//! torchflower crop).
use super::prelude::*;
use super::vegetation_block::{self, MayPlaceOn};

pub struct CropBlock;

/// `CropBlock.hasSufficientLight`.
pub fn has_sufficient_light(level: &dyn LevelReader, pos: BlockPos) -> bool {
    level.get_raw_brightness(pos, 0) >= 8
}

impl MayPlaceOn for CropBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_crops")
    }
}

impl BlockBehaviour for CropBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        has_sufficient_light(level, pos) && vegetation_block::can_survive(self, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        vegetation_block::update_shape(state, &*level, pos)
    });
}

register_block_class!("CropBlock", |_| Box::new(CropBlock));
