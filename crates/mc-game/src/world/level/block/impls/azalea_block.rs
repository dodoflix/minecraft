//! `net.minecraft.world.level.block.AzaleaBlock` (extends `VegetationBlock`).
use super::prelude::*;
use super::vegetation_block::{MayPlaceOn, vegetation_behaviour};

pub struct AzaleaBlock;

impl MayPlaceOn for AzaleaBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_azalea")
    }
}
vegetation_behaviour!(AzaleaBlock);

register_block_class!("AzaleaBlock", |_| Box::new(AzaleaBlock));
