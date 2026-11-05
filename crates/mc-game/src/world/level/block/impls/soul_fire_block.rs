//! `net.minecraft.world.level.block.SoulFireBlock`.
use super::prelude::*;

pub struct SoulFireBlock;

/// `SoulFireBlock.canSurviveOnBlock`.
pub fn can_survive_on_block(state: BlockState) -> bool {
    state.is_tag("soul_fire_base_blocks")
}

impl BlockBehaviour for SoulFireBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        can_survive_on_block(level.get_block_state(pos.below()))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if self.can_survive(state, &*level, pos) { Block::SOUL_FIRE.default_state() } else { air() }
    });
}

register_block_class!("SoulFireBlock", |_| Box::new(SoulFireBlock));
