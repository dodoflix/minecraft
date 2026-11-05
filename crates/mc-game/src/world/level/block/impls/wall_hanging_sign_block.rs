//! `net.minecraft.world.level.block.WallHangingSignBlock` (extends `SignBlock`).
use super::prelude::*;
use super::sign_block;

pub struct WallHangingSignBlock;

impl BlockBehaviour for WallHangingSignBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction.get_axis() == state.get(props::HORIZONTAL_FACING).get_clockwise().get_axis() && !state.can_survive(&*level, pos) {
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

register_block_class!("WallHangingSignBlock", |_| Box::new(WallHangingSignBlock));
