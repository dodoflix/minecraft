//! `net.minecraft.world.level.block.ShelfBlock`.
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::prelude::*;

pub struct ShelfBlock;

impl BlockBehaviour for ShelfBlock {
    mc_game_core::simple_waterlogged!();
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("ShelfBlock", |_| Box::new(ShelfBlock));
