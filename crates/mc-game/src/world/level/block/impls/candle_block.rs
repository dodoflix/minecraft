//! `net.minecraft.world.level.block.CandleBlock` (extends `AbstractCandleBlock`).
use super::prelude::*;

pub struct CandleBlock;

impl BlockBehaviour for CandleBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        helpers::can_support_center(level, pos.below(), Direction::Up)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
}

register_block_class!("CandleBlock", |_| Box::new(CandleBlock));
