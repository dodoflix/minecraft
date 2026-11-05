//! `net.minecraft.world.level.block.WallSignBlock` (extends `SignBlock`).
use super::prelude::*;
use super::sign_block;

pub struct WallSignBlock;

impl BlockBehaviour for WallSignBlock {
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.relative(state.get(props::HORIZONTAL_FACING).get_opposite())).is_solid()
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction.get_opposite() == state.get(props::HORIZONTAL_FACING) && !state.can_survive(&*level, pos) {
            air()
        } else {
            sign_block::update_shape(state, level, pos)
        }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("WallSignBlock", |_| Box::new(WallSignBlock));
