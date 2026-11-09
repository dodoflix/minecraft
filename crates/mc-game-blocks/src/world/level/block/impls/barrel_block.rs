//! `net.minecraft.world.level.block.BarrelBlock`.
use mc_game_core::register_block_class;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::block_entity::barrel_block_entity::BarrelBlockEntity;
use mc_game_core::world::container::get_redstone_signal_from_block_entity;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::impls::prelude::*;

pub struct BarrelBlock;

impl BlockBehaviour for BarrelBlock {
    fn get_state_for_placement(
        &self,
        block: Block,
        _level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        Some(block.default_state().set(
            props::FACING,
            context.get_nearest_looking_direction().get_opposite(),
        ))
    }
    fn get_menu_provider(
        &self,
        _state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
    ) -> Option<Box<dyn mc_game_core::world::inventory::MenuProvider>> {
        use mc_game_core::world::block_entity::container_menus::{
            BlockEntityMenuProvider, ContainerBlockKind,
        };
        let title = level
            .get_block_entity(pos)
            .and_then(|be| be.as_any().downcast_ref::<BarrelBlockEntity>())
            .map(|b| b.get_display_name())?;
        Some(Box::new(BlockEntityMenuProvider {
            kind: ContainerBlockKind::Barrel,
            first: pos,
            second: None,
            title,
        }))
    }
    fn use_without_item(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        player: &mut mc_game_core::world::entity::ServerPlayer,
        _hit: &mc_core::phys::BlockHitResult,
    ) -> mc_game_core::world::item::InteractionResult {
        if let Some(provider) = self.get_menu_provider(state, level, pos) {
            player.open_menu(level, provider.as_ref());
            player.award_stat_once(&*level, mc_game_core::stats::stats::Stats::OPEN_BARREL);
            mc_game_core::server::level::block_break::anger_nearby_piglins(level, &*player, true);
        }
        mc_game_core::world::item::InteractionResult::SUCCESS
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        // `Containers.updateNeighboursAfterDestroy`
        level.update_neighbour_for_output_signal(pos, state.block());
    }
    fn tick(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        level.with_block_entity(pos, |level, be| {
            if let Some(barrel) = be.as_any_mut().downcast_mut::<BarrelBlockEntity>() {
                barrel.recheck_open(level);
            }
        });
    }
    fn get_analog_output_signal(
        &self,
        _state: BlockState,
        level: &ServerLevel,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        get_redstone_signal_from_block_entity(level.get_block_entity(pos))
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING, mirror)
    }
}

register_block_class!("BarrelBlock", |_| Box::new(BarrelBlock));
