//! `net.minecraft.world.level.block.LilyPadBlock` (extends `VegetationBlock`).
use super::prelude::*;
use super::vegetation_block::{MayPlaceOn, vegetation_behaviour};

pub struct LilyPadBlock;

impl MayPlaceOn for LilyPadBlock {
    fn may_place_on(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
        let fluid_state = level.get_fluid_state(pos);
        let fluid_above = level.get_fluid_state(pos.above());
        (fluid_state.is_tag("supports_lily_pad") || state.is_tag("supports_lily_pad")) && fluid_above.is_empty()
    }
}
vegetation_behaviour!(LilyPadBlock);

register_block_class!("LilyPadBlock", |_| Box::new(LilyPadBlock));
