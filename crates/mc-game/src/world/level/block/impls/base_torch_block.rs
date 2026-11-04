//! `net.minecraft.world.level.block.BaseTorchBlock` (also `TorchBlock`, `RedstoneTorchBlock`: no own overrides here).
use super::prelude::*;

pub struct BaseTorchBlock;

impl BlockBehaviour for BaseTorchBlock {
    fn update_shape(
        &self,
        state: BlockState,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        direction_to_neighbour: Direction,
        _neighbour_pos: BlockPos,
        _neighbour_state: BlockState,
    ) -> BlockState {
        if direction_to_neighbour == Direction::Down && !self.can_survive(state, &*level, pos) { air() } else { state }
    }
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        helpers::can_support_center(level, pos.below(), Direction::Up)
    }
}

register_block_class!("BaseTorchBlock", |_| Box::new(BaseTorchBlock));
