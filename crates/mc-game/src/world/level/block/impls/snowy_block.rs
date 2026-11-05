//! `net.minecraft.world.level.block.SnowyBlock` (also `SpreadingSnowyBlock`: grass block, mycelium).
use super::prelude::*;

pub struct SnowyBlock;

impl BlockBehaviour for SnowyBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if direction == Direction::Up { state.set(props::SNOWY, neighbour_state.is_tag("snow")) } else { state }
    });
}

register_block_class!("SnowyBlock", |_| Box::new(SnowyBlock));
