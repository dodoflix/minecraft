//! `net.minecraft.world.level.block.GrindstoneBlock` (extends `FaceAttachedHorizontalDirectionalBlock`).
use mc_core::phys::BlockHitResult;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::stats::stats::Stats;
use mc_game_core::world::entity::ServerPlayer;
use mc_game_core::world::inventory::{
    ContainerLevelAccess, GrindstoneMenu, MenuProvider, SimpleMenuProvider,
};
use mc_game_core::world::item::{BlockPlaceContext, InteractionResult};
use mc_game_core::world::level::block::impls::face_attached_horizontal_directional_block as face_attached;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::{register_block_class, update_shape};
use mc_text::Component;

pub struct GrindstoneBlock;

impl BlockBehaviour for GrindstoneBlock {
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        face_attached::get_state_for_placement(block, &*level, context)
    }
    fn can_survive(&self, _state: BlockState, _level: &dyn LevelReader, _pos: BlockPos) -> bool {
        true
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            // `FaceAttachedHorizontalDirectionalBlock.updateShape` (never removes: `canSurvive` is always true)
            state
        }
    );
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
    /// `useWithoutItem`: `player.openMenu(state.getMenuProvider(level, pos))` + `INTERACT_WITH_GRINDSTONE`.
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
        player.award_stat_once(level, Stats::INTERACT_WITH_GRINDSTONE);
        InteractionResult::SUCCESS
    }
    fn get_menu_provider(
        &self,
        _state: BlockState,
        _level: &ServerLevel,
        pos: BlockPos,
    ) -> Option<Box<dyn MenuProvider>> {
        Some(Box::new(SimpleMenuProvider::new(
            Component::translatable("container.grindstone_title", Vec::new()),
            move |container_id, player| {
                Some(Box::new(GrindstoneMenu::new(
                    container_id,
                    player,
                    ContainerLevelAccess::create(pos),
                )))
            },
        )))
    }
}

register_block_class!("GrindstoneBlock", |_| Box::new(GrindstoneBlock));
