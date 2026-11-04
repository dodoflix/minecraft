//! `net.minecraft.world.level.block.BambooStalkBlock`.
use super::prelude::*;

pub struct BambooStalkBlock;

impl BlockBehaviour for BambooStalkBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.below()).is_tag("supports_bamboo")
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !state.can_survive(&*level, pos) {
            helpers::schedule_block_tick(level, pos, state.block(), 1);
        }
        if direction == Direction::Up && neighbour_state.is_block(Block::BAMBOO) && neighbour_state.get(props::AGE_1) > state.get(props::AGE_1) {
            state.cycle(props::AGE_1)
        } else {
            state
        }
    });
}

register_block_class!("BambooStalkBlock", |_| Box::new(BambooStalkBlock));
