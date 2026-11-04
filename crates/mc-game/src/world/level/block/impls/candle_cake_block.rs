//! `net.minecraft.world.level.block.CandleCakeBlock` (extends `AbstractCandleBlock`).
use super::prelude::*;

pub struct CandleCakeBlock;

impl BlockBehaviour for CandleCakeBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.below()).is_solid()
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Down && !state.can_survive(&*level, pos) { air() } else { state }
    });
}

register_block_class!("CandleCakeBlock", |_| Box::new(CandleCakeBlock));
