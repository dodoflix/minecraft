//! `net.minecraft.world.level.block.StemBlock` (extends `VegetationBlock`): the support tag depends on the block.
use super::prelude::*;
use super::vegetation_block::{MayPlaceOn, vegetation_behaviour};

pub struct StemBlock {
    support_blocks: &'static str,
}

impl MayPlaceOn for StemBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag(self.support_blocks)
    }
}
vegetation_behaviour!(StemBlock);

register_block_class!("StemBlock", |block| Box::new(StemBlock {
    support_blocks: match block.data().name {
            "minecraft:pumpkin_stem" => "supports_pumpkin_stem",
            "minecraft:melon_stem" => "supports_melon_stem",
            name => panic!("unexpected StemBlock {name}"),
        },
}));
