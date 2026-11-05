//! `net.minecraft.world.level.block.LightningRodBlock` (extends `RodBlock`).
use super::prelude::*;

pub struct LightningRodBlock;

impl BlockBehaviour for LightningRodBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        state.set(props::FACING, mirror.mirror(state.get(props::FACING)))
    }
}

register_block_class!("LightningRodBlock", |_| Box::new(LightningRodBlock));
