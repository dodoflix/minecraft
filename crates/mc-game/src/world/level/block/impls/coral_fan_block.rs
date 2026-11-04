//! `net.minecraft.world.level.block.CoralFanBlock` (extends `BaseCoralFanBlock`).
use super::base_coral_plant_type_block as base;
use super::prelude::*;

pub struct CoralFanBlock;

impl BlockBehaviour for CoralFanBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        base::can_survive(level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Down && !state.can_survive(&*level, pos) {
            air()
        } else {
            base::try_schedule_die_tick(state, level, pos);
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            base::update_shape(state, level, pos, direction)
        }
    });
}

register_block_class!("CoralFanBlock", |_| Box::new(CoralFanBlock));
