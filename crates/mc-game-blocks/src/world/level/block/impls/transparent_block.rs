//! `net.minecraft.world.level.block.TransparentBlock` (glass, stained glass, tinted glass): nothing to see for `ClipContext.Block.VISUAL`.
use mc_core::phys::shapes::{Shapes, VoxelShape};
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::collision::CollisionContext;

pub struct TransparentBlock;

impl BlockBehaviour for TransparentBlock {
    fn get_visual_shape(
        &self,
        _state: BlockState,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
        _context: &CollisionContext,
    ) -> VoxelShape {
        Shapes::empty()
    }
}

register_block_class!("TransparentBlock", |_| Box::new(TransparentBlock));
