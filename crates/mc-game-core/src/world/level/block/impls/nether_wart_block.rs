//! `net.minecraft.world.level.block.NetherWartBlock` (extends `VegetationBlock`).
use super::prelude::*;
use super::vegetation_block::{MayPlaceOn, vegetation_behaviour_methods};
use crate::server::level::ServerLevel;
use crate::world::item::ItemStack;

pub struct NetherWartBlock;

impl MayPlaceOn for NetherWartBlock {
    fn may_place_on(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> bool {
        state.is_tag("supports_nether_wart")
    }
}

impl BlockBehaviour for NetherWartBlock {
    vegetation_behaviour_methods!();
    fn random_tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let age = state.get(props::AGE_3);
        if age < 3 && level.random().next_int_bounded(10) == 0 {
            level.set_block_flags(pos, state.set(props::AGE_3, age + 1), 2);
        }
    }
    fn get_clone_item_stack(&self, _state: BlockState, _level: &ServerLevel, _pos: BlockPos, _include_data: bool) -> ItemStack {
        ItemStack::new(mc_data::Item::NETHER_WART)
    }
}

register_block_class!("NetherWartBlock", |_| Box::new(NetherWartBlock));
