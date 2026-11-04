//! `net.minecraft.world.level.block.NetherFungusBlock` (extends `VegetationBlock`): the support tag depends on the block.
use super::prelude::*;
use super::vegetation_block::{MayPlaceOn, vegetation_behaviour};

pub struct NetherFungusBlock {
    support_blocks: &'static str,
}

impl MayPlaceOn for NetherFungusBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag(self.support_blocks)
    }
}
vegetation_behaviour!(NetherFungusBlock);

register_block_class!("NetherFungusBlock", |block| Box::new(NetherFungusBlock {
    support_blocks: match block.data().name {
            "minecraft:warped_fungus" => "supports_warped_fungus",
            "minecraft:crimson_fungus" => "supports_crimson_fungus",
            name => panic!("unexpected NetherFungusBlock {name}"),
        },
}));
