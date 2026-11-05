//! `net.minecraft.world.level.block.FlowerPotBlock`.
use super::prelude::*;

pub struct FlowerPotBlock;

impl BlockBehaviour for FlowerPotBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Down && !state.can_survive(&*level, pos) { air() } else { state }
    });
}

register_block_class!("FlowerPotBlock", |_| Box::new(FlowerPotBlock));
