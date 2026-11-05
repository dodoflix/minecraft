//! `net.minecraft.world.level.block.EnderChestBlock` (extends `AbstractChestBlock`).
use super::prelude::*;

pub struct EnderChestBlock;

impl BlockBehaviour for EnderChestBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("EnderChestBlock", |_| Box::new(EnderChestBlock));
