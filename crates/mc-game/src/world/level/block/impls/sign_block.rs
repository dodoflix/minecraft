//! `net.minecraft.world.level.block.SignBlock` (abstract base of the sign blocks).
use super::prelude::*;

/// `SignBlock.updateShape`.
pub fn update_shape(state: BlockState, level: &mut dyn LevelAccessor, pos: BlockPos) -> BlockState {
    if state.get(props::WATERLOGGED) {
        helpers::schedule_water_tick(level, pos);
    }
    state
}
