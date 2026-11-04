//! `net.minecraft.world.level.block.NetherWartBlock` (extends `VegetationBlock`).
use super::prelude::*;
use super::vegetation_block::{MayPlaceOn, vegetation_behaviour};

pub struct NetherWartBlock;

impl MayPlaceOn for NetherWartBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_nether_wart")
    }
}
vegetation_behaviour!(NetherWartBlock);

register_block_class!("NetherWartBlock", |_| Box::new(NetherWartBlock));
