//! `net.minecraft.world.level.block.SeagrassBlock` (extends `VegetationBlock`).
use super::prelude::*;
use super::vegetation_block::{self, MayPlaceOn};

pub struct SeagrassBlock;

impl MayPlaceOn for SeagrassBlock {
    fn may_place_on(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        state.is_face_sturdy_at(level, pos, Direction::Up) && !state.is_tag("cannot_support_seagrass")
    }
}

impl BlockBehaviour for SeagrassBlock {
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
