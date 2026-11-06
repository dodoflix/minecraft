//! `net.minecraft.world.level.block.RodBlock` (extends `DirectionalBlock`; end rod, lightning rod). Subclasses that register
//! their own behavior (`LightningRodBlock`) delegate to `RodBlock` for the methods they do not override.
use mc_data::block::PathComputationType;
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct RodBlock;

impl BlockBehaviour for RodBlock {
    fn is_pathfindable(&self, _state: BlockState, _ty: PathComputationType) -> bool {
        false
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        state.set(props::FACING, mirror.mirror(state.get(props::FACING)))
    }
}

register_block_class!("RodBlock", |_| Box::new(RodBlock));
