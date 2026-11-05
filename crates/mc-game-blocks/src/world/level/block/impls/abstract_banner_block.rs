//! `net.minecraft.world.level.block.AbstractBannerBlock`: shared by `BannerBlock` and `WallBannerBlock`.
use mc_game_core::server::level::ServerLevel;
use crate::world::block_entity::BannerBlockEntity;
use mc_game_core::world::item::ItemStack;
use mc_game_core::world::level::BlockGetter;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;

/// `getCloneItemStack(level, pos, state, includeData)`: the banner block entity's item (with patterns and name), else
/// `super.getCloneItemStack`.
pub fn get_clone_item_stack(state: BlockState, level: &ServerLevel, pos: BlockPos) -> ItemStack {
    if let Some(banner) = level.get_block_entity(pos).and_then(|be| be.as_any().downcast_ref::<BannerBlockEntity>()) {
        return banner.get_item();
    }
    state.block().item().map_or_else(ItemStack::empty, ItemStack::new)
}
