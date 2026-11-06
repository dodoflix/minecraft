//! `net.minecraft.world.level.block.NetherSproutsBlock` (extends `VegetationBlock`).
use mc_game_core::register_block_class;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::vegetation_block::{
    MayPlaceOn, vegetation_behaviour,
};

pub struct NetherSproutsBlock;

impl MayPlaceOn for NetherSproutsBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_nether_sprouts")
    }
}
vegetation_behaviour!(NetherSproutsBlock);

register_block_class!("NetherSproutsBlock", |_| Box::new(NetherSproutsBlock));
