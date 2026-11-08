//! `net.minecraft.world.level.block.EnchantingTableBlock` (extends `BaseEntityBlock`): opens the enchantment menu titled
//! with the block entity's name. The shape and the light occlusion come from the block data.
use crate::world::block_entity::enchanting_table_block_entity::EnchantingTableBlockEntity;
use mc_core::phys::BlockHitResult;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::inventory::{
    ContainerLevelAccess, EnchantmentMenu, MenuProvider, SimpleMenuProvider,
};
use mc_game_core::world::item::InteractionResult;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct EnchantingTableBlock;

impl BlockBehaviour for EnchantingTableBlock {
    /// `useWithoutItem`: `player.openMenu(state.getMenuProvider(level, pos))`.
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
        InteractionResult::SUCCESS
    }

    /// `getMenuProvider`: the enchantment menu, titled with the block entity's display name.
    fn get_menu_provider(
        &self,
        _state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
    ) -> Option<Box<dyn MenuProvider>> {
        let title = level
            .get_block_entity(pos)?
            .as_any()
            .downcast_ref::<EnchantingTableBlockEntity>()?
            .get_name();
        Some(Box::new(SimpleMenuProvider::new(
            title,
            move |container_id, player| {
                Some(Box::new(EnchantmentMenu::new(
                    container_id,
                    player,
                    ContainerLevelAccess::create(pos),
                )))
            },
        )))
    }

    fn is_pathfindable(
        &self,
        _state: BlockState,
        _ty: mc_data::block::PathComputationType,
    ) -> bool {
        false
    }
}

register_block_class!("EnchantingTableBlock", |_| Box::new(EnchantingTableBlock));
