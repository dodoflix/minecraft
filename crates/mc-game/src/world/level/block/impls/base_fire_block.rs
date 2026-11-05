//! `net.minecraft.world.level.block.BaseFireBlock` (the static helpers; `FireBlock`/`SoulFireBlock` extend it).
use super::prelude::*;

/// `BaseFireBlock.getState(level, pos)`: soul fire on soul-fire base blocks, else `FireBlock.getStateForPlacement`.
pub fn get_state(level: &dyn BlockGetter, pos: BlockPos) -> BlockState {
    if super::soul_fire_block::can_survive_on_block(level.get_block_state(pos.below())) {
        Block::SOUL_FIRE.default_state()
    } else {
        super::fire_block::get_state_for_placement(level, pos)
    }
}
