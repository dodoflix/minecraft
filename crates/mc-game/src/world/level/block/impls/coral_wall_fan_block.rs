//! `net.minecraft.world.level.block.CoralWallFanBlock` (extends `BaseCoralWallFanBlock`).
use super::base_coral_plant_type_block as base;
use super::base_coral_wall_fan_block as wall_fan;
use super::prelude::*;

pub struct CoralWallFanBlock;

impl BlockBehaviour for CoralWallFanBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        wall_fan::can_survive(state, level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction.get_opposite() == state.get(props::HORIZONTAL_FACING) && !state.can_survive(&*level, pos) {
            air()
        } else {
            if state.get(props::WATERLOGGED) {
                helpers::schedule_water_tick(level, pos);
            }
            base::try_schedule_die_tick(state, level, pos);
            wall_fan::update_shape(state, level, pos, direction)
        }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("CoralWallFanBlock", |_| Box::new(CoralWallFanBlock));
