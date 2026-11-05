//! `net.minecraft.world.level.block.StonecutterBlock`: `rotate` / `mirror` of the `facing` property.
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct StonecutterBlock;

impl BlockBehaviour for StonecutterBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("StonecutterBlock", |_| Box::new(StonecutterBlock));
