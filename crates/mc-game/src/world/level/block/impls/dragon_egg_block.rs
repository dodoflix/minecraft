//! `net.minecraft.world.level.block.DragonEggBlock` (extends `FallingBlock`, `getDelayAfterPlace` = 5).
use super::prelude::*;
use super::falling_block;
use crate::server::level::ServerLevel;

pub struct DragonEggBlock;

impl BlockBehaviour for DragonEggBlock {
    fn on_place(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _old_state: BlockState, _moved_by_piston: bool) {
        falling_block::on_place(state, level, pos, 5);
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        falling_block::tick(state, level, pos);
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        falling_block::update_shape(state, level, pos, 5)
    });
}

register_block_class!("DragonEggBlock", |_| Box::new(DragonEggBlock));
