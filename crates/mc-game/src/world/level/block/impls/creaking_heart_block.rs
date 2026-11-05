//! `net.minecraft.world.level.block.CreakingHeartBlock`.
use super::prelude::*;
use super::rotated_pillar_block::rotate_pillar;

pub struct CreakingHeartBlock;

impl BlockBehaviour for CreakingHeartBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        helpers::schedule_block_tick(level, pos, state.block(), 1);
        state
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        rotate_pillar(state, rotation)
    }
}

register_block_class!("CreakingHeartBlock", |_| Box::new(CreakingHeartBlock));
