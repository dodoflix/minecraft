//! `net.minecraft.world.level.block.CakeBlock`.
use super::prelude::*;

pub struct CakeBlock;

impl BlockBehaviour for CakeBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.below()).is_solid()
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Down && !state.can_survive(&*level, pos) { air() } else { state }
    });
}

register_block_class!("CakeBlock", |_| Box::new(CakeBlock));
