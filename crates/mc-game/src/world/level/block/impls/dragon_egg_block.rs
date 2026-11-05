//! `net.minecraft.world.level.block.DragonEggBlock` (extends `FallingBlock`, `getDelayAfterPlace` = 5).
use super::prelude::*;
use super::falling_block;

pub struct DragonEggBlock;

impl BlockBehaviour for DragonEggBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        falling_block::update_shape(state, level, pos, 5)
    });
}

register_block_class!("DragonEggBlock", |_| Box::new(DragonEggBlock));
