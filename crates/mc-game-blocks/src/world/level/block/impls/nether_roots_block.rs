//! `net.minecraft.world.level.block.NetherRootsBlock` (extends `VegetationBlock`): the support tag depends on the block.
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::{MayPlaceOn, vegetation_behaviour};

pub struct NetherRootsBlock {
    support_blocks: &'static str,
}

impl MayPlaceOn for NetherRootsBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag(self.support_blocks)
    }
}
vegetation_behaviour!(NetherRootsBlock);

register_block_class!("NetherRootsBlock", |block| Box::new(NetherRootsBlock {
    support_blocks: match block.data().name {
            "minecraft:warped_roots" => "supports_warped_roots",
            "minecraft:crimson_roots" => "supports_crimson_roots",
            name => panic!("unexpected NetherRootsBlock {name}"),
        },
}));
