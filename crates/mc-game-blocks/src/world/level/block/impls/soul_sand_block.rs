//! `net.minecraft.world.level.block.SoulSandBlock`: the visual shape is a full block (its collision shape is lower).
use mc_core::phys::shapes::{Shapes, VoxelShape};
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::collision::CollisionContext;

pub struct SoulSandBlock;

impl BlockBehaviour for SoulSandBlock {
    fn get_visual_shape(
        &self,
        _state: BlockState,
        _level: &dyn BlockGetter,
        _pos: BlockPos,
        _context: &CollisionContext,
    ) -> VoxelShape {
        Shapes::block()
    }
}

register_block_class!("SoulSandBlock", |_| Box::new(SoulSandBlock));
