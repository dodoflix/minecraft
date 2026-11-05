//! `net.minecraft.world.level.block.CactusFlowerBlock` (extends `VegetationBlock`).
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::{MayPlaceOn, vegetation_behaviour};

pub struct CactusFlowerBlock;

impl MayPlaceOn for CactusFlowerBlock {
    fn may_place_on(&self, _state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        // vanilla reads the block at `pos` (the position below the flower) here, not the passed state
        let block_below = level.get_block_state(pos);
        block_below.is_tag("support_override_cactus_flower") || block_below.is_face_sturdy_support(level, pos, Direction::Up, SupportType::Center)
    }
}
vegetation_behaviour!(CactusFlowerBlock);

register_block_class!("CactusFlowerBlock", |_| Box::new(CactusFlowerBlock));
