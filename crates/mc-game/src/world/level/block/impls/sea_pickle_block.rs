//! `net.minecraft.world.level.block.SeaPickleBlock` (extends `VegetationBlock`).
use super::prelude::*;
use super::vegetation_block::MayPlaceOn;

pub struct SeaPickleBlock;

impl MayPlaceOn for SeaPickleBlock {
    fn may_place_on(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        !state.get_collision_shape(level, pos).get_face_shape(Direction::Up).is_empty() || state.is_face_sturdy_at(level, pos, Direction::Up)
    }
}

impl BlockBehaviour for SeaPickleBlock {
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
