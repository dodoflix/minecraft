//! `net.minecraft.world.level.block.StonecutterBlock`: placement, `rotate` / `mirror` of the `facing` property,
//! `useWithoutItem` (menu, stat and success) and the menu provider.
use mc_core::phys::BlockHitResult;
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::stats::stats::Stats;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::inventory::{
    ContainerLevelAccess, MenuProvider, SimpleMenuProvider, StonecutterMenu,
};
use mc_game_core::world::item::{BlockPlaceContext, InteractionResult};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_text::Component;

pub struct StonecutterBlock;

impl BlockBehaviour for StonecutterBlock {
    fn get_state_for_placement(
        &self,
        block: Block,
        _level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(block.default_state().set(
            props::HORIZONTAL_FACING,
            context.get_horizontal_direction().get_opposite(),
        ))
    }
    /// `useWithoutItem`: `player.openMenu(state.getMenuProvider(level, pos))` + `INTERACT_WITH_STONECUTTER`.
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
        player.award_stat_once(level, Stats::INTERACT_WITH_STONECUTTER);
        InteractionResult::SUCCESS
    }
    fn get_menu_provider(
        &self,
        _state: BlockState,
        _level: &ServerLevel,
        pos: BlockPos,
    ) -> Option<Box<dyn MenuProvider>> {
        Some(Box::new(SimpleMenuProvider::new(
            Component::translatable("container.stonecutter", Vec::new()),
            move |container_id, player| {
                Some(Box::new(StonecutterMenu::new(
                    container_id,
                    player,
                    ContainerLevelAccess::create(pos),
                )))
            },
        )))
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("StonecutterBlock", |_| Box::new(StonecutterBlock));
