//! `net.minecraft.world.level.block.WitherRoseBlock` (extends `VegetationBlock`).
use super::prelude::*;
use super::vegetation_block::{MayPlaceOn, vegetation_behaviour};

pub struct WitherRoseBlock;

impl MayPlaceOn for WitherRoseBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_wither_rose")
    }
}
vegetation_behaviour!(WitherRoseBlock);

register_block_class!("WitherRoseBlock", |_| Box::new(WitherRoseBlock));
