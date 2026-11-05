//! `net.minecraft.world.level.block.CampfireBlock`.
use super::prelude::*;

pub struct CampfireBlock;

impl BlockBehaviour for CampfireBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        if direction == Direction::Down { state.set(props::SIGNAL_FIRE, neighbour_state.is_block(Block::HAY_BLOCK)) } else { state }
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("CampfireBlock", |_| Box::new(CampfireBlock));
