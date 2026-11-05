//! `net.minecraft.world.level.block.FallingBlock` (sand, gravel, concrete powder, anvil, dragon egg).
use super::prelude::*;

pub struct FallingBlock;

/// `FallingBlock.updateShape` (`delay` = `getDelayAfterPlace()`).
pub fn update_shape(state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos, delay: i32) -> BlockState {
    helpers::schedule_block_tick(level, pos, state.block(), delay);
    state
}

impl BlockBehaviour for FallingBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        update_shape(state, level, pos, 2)
    });
}

register_block_class!("FallingBlock", |_| Box::new(FallingBlock));
