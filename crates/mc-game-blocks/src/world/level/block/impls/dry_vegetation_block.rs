//! `net.minecraft.world.level.block.DryVegetationBlock` (extends `VegetationBlock`).
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::{MayPlaceOn, vegetation_behaviour};

pub struct DryVegetationBlock;

impl MayPlaceOn for DryVegetationBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_dry_vegetation")
    }
}
vegetation_behaviour!(DryVegetationBlock);

register_block_class!("DryVegetationBlock", |_| Box::new(DryVegetationBlock));
