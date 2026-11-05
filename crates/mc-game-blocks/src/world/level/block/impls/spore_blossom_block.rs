//! `net.minecraft.world.level.block.SporeBlossomBlock`.
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::prelude::*;

pub struct SporeBlossomBlock;

impl BlockBehaviour for SporeBlossomBlock {
    fn can_survive(&self, _state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        helpers::can_support_center(level, pos.above(), Direction::Down) && !helpers::is_water_at(level, pos)
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Up && !self.can_survive(state, &*level, pos) { air() } else { state }
    });
}

register_block_class!("SporeBlossomBlock", |_| Box::new(SporeBlossomBlock));
