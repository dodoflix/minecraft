//! `net.minecraft.world.level.block.EnderChestBlock` (extends `AbstractChestBlock`).
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::block_entity::ender_chest_block_entity::EnderChestBlockEntity;
use crate::world::item::BlockPlaceContext;
use mc_data::fluid::Fluid;

pub struct EnderChestBlock;

impl BlockBehaviour for EnderChestBlock {
    crate::simple_waterlogged!();
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let replaced_fluid_state = level.get_fluid_state(context.get_clicked_pos());
        Some(block.default_state().set(props::HORIZONTAL_FACING, context.get_horizontal_direction().get_opposite()).set(props::WATERLOGGED, replaced_fluid_state.fluid() == Fluid::WATER))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
    fn use_without_item(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut crate::world::entity::ServerPlayer, _hit: &mc_core::phys::BlockHitResult) -> crate::world::item::InteractionResult {
        use crate::world::inventory::{ChestMenu, ContainerRef, SimpleMenuProvider};
        if level.get_block_entity(pos).is_some_and(|be| be.as_any().is::<EnderChestBlockEntity>()) {
            if level.get_block_state(pos.above()).is_redstone_conductor() {
                return crate::world::item::InteractionResult::SUCCESS;
            }
            player.player.ender_chest.set_active_chest(pos);
            let provider = SimpleMenuProvider::new(mc_text::Component::translatable("container.enderchest", vec![]), |id, p: &mut dyn crate::world::inventory::MenuPlayer| {
                Some(Box::new(ChestMenu::three_rows(id, p, ContainerRef::player_ender_chest())) as Box<dyn crate::world::inventory::AbstractContainerMenu>)
            });
            if player.open_menu(level, &provider).is_some() {
                player.award_stat_once(&*level, crate::stats::stats::Stats::OPEN_ENDERCHEST);
            }
        }
        crate::world::item::InteractionResult::SUCCESS
    }
    fn trigger_event(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, b0: i32, b1: i32) -> bool {
        helpers::base_entity_trigger_event(level, pos, b0, b1)
    }
    fn tick(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        level.with_block_entity(pos, |level, be| {
            if let Some(chest) = be.as_any_mut().downcast_mut::<EnderChestBlockEntity>() {
                chest.recheck_open(level);
            }
        });
    }
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("EnderChestBlock", |_| Box::new(EnderChestBlock));
