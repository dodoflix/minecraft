//! `net.minecraft.world.level.block.BambooSaplingBlock`.
use super::prelude::*;

pub struct BambooSaplingBlock;

impl BlockBehaviour for BambooSaplingBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        level.get_block_state(pos.below()).is_tag("supports_bamboo")
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if !state.can_survive(&*level, pos) {
            air()
        } else if direction == Direction::Up && neighbour_state.is_block(Block::BAMBOO) {
            Block::BAMBOO.default_state()
        } else {
            state
        }
    });
}

register_block_class!("BambooSaplingBlock", |_| Box::new(BambooSaplingBlock));
