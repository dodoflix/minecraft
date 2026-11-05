//! `net.minecraft.world.level.block.BarrelBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::block_entity::barrel_block_entity::BarrelBlockEntity;
use crate::world::container::get_redstone_signal_from_block_entity;
use crate::world::item::BlockPlaceContext;

pub struct BarrelBlock;

impl BlockBehaviour for BarrelBlock {
    fn get_state_for_placement(&self, block: Block, _level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(block.default_state().set(props::FACING, context.get_nearest_looking_direction().get_opposite()))
    }
    fn use_without_item(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut crate::world::entity::ServerPlayer, _hit: &mc_core::phys::BlockHitResult) -> crate::world::item::InteractionResult {
        use crate::world::block_entity::container_menus::{BlockEntityMenuProvider, ContainerBlockKind};
        if let Some(title) = level.get_block_entity(pos).and_then(|be| be.as_any().downcast_ref::<BarrelBlockEntity>()).map(|b| b.get_display_name()) {
            let provider = BlockEntityMenuProvider { kind: ContainerBlockKind::Barrel, first: pos, second: None, title };
            if player.open_menu(level, &provider).is_some() {
                player.award_stat_once(&*level, crate::stats::stats::Stats::OPEN_BARREL);
            }
        }
        crate::world::item::InteractionResult::SUCCESS
    }
    fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _moved_by_piston: bool) {
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
    fn get_analog_output_signal(&self, _state: BlockState, level: &ServerLevel, pos: BlockPos, _direction: Direction) -> i32 {
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
