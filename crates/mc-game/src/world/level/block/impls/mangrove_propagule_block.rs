//! `net.minecraft.world.level.block.MangrovePropaguleBlock` (extends `SaplingBlock`).
use super::prelude::*;
use super::vegetation_block::{self, MayPlaceOn};

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

impl BlockBehaviour for MangrovePropaguleBlock {
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
