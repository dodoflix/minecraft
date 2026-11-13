//! `net.minecraft.world.level.block.CraftingTableBlock`: `useWithoutItem` (stat and success), `getMenuProvider`
//! (`container.crafting`).
use mc_core::phys::BlockHitResult;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::stats::stats::Stats;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::inventory::{
    ContainerLevelAccess, CraftingMenu, MenuProvider, SimpleMenuProvider,
};
use mc_game_core::world::item::InteractionResult;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_text::Component;

pub struct CraftingTableBlock;

impl BlockBehaviour for CraftingTableBlock {
    /// `useWithoutItem`: `player.openMenu(state.getMenuProvider(level, pos))` + `INTERACT_WITH_CRAFTING_TABLE`.
    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut ServerPlayer,
        _hit: &BlockHitResult,
    ) -> InteractionResult {
        if let Some(provider) = self.get_menu_provider(state, level, pos) {
            player.open_menu(level, provider.as_ref());
        }
        player.award_stat_once(level, Stats::INTERACT_WITH_CRAFTING_TABLE);
        InteractionResult::SUCCESS
    }
    fn get_menu_provider(
        &self,
        _state: BlockState,
        _level: &ServerLevel,
        pos: BlockPos,
    ) -> Option<Box<dyn MenuProvider>> {
        Some(Box::new(SimpleMenuProvider::new(
            Component::translatable("container.crafting", Vec::new()),
            move |container_id, _player| {
                Some(Box::new(CraftingMenu::new(
                    container_id,
                    ContainerLevelAccess::create(pos),
                )))
            },
        )))
    }
}

register_block_class!("CraftingTableBlock", |_| Box::new(CraftingTableBlock));
