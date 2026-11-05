//! `net.minecraft.world.level.block.HopperBlock`.
use super::dispenser_block::update_neighbours_after_destroy;
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::block_entity::hopper_block_entity;
use crate::world::entity::Entity;
use crate::world::entity::item::ItemEntity;
use crate::world::item::BlockPlaceContext;
use crate::world::level::redstone::Orientation;
use crate::world::level::signal_getter::has_neighbor_signal;
use crate::stats::stats::Stats;
use crate::world::block_entity::container_menu_provider::BlockEntityMenuProvider;
use crate::world::entity::ServerPlayer;
use crate::world::item::InteractionResult;
use mc_core::phys::BlockHitResult;
use mc_data::block::PathComputationType;

pub struct HopperBlock;

/// `HopperBlock.checkPoweredState(level, pos, state)`.
fn check_powered_state(level: &mut ServerLevel, pos: BlockPos, state: BlockState) {
    let should_be_on = !has_neighbor_signal(level, pos);
    if should_be_on != state.get(props::ENABLED) {
        level.set_block_flags(pos, state.set(props::ENABLED, should_be_on), 2);
    }
}

impl BlockBehaviour for HopperBlock {
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING_HOPPER, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::FACING_HOPPER, mirror)
    }

    fn get_state_for_placement(&self, block: Block, _level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        let direction = context.get_clicked_face().get_opposite();
        let facing = if direction.get_axis() == Axis::Y { Direction::Down } else { direction };
        Some(block.default_state().set(props::FACING_HOPPER, facing).set(props::ENABLED, true))
    }

    fn on_place(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, old_state: BlockState, _moved_by_piston: bool) {
        if !old_state.is_block(state.block()) {
            check_powered_state(level, pos, state);
        }
    }

    fn use_without_item(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, player: &mut ServerPlayer, _hit: &BlockHitResult) -> InteractionResult {
        if let Some(provider) = BlockEntityMenuProvider::of(level, pos) {
            player.open_menu(level, &provider);
            player.award_stat_once(level, Stats::INSPECT_HOPPER);
        }
        InteractionResult::SUCCESS
    }

    fn neighbor_changed(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _block: Block, _orientation: Option<Orientation>, _moved_by_piston: bool) {
        check_powered_state(level, pos, state);
    }

    fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _moved_by_piston: bool) {
        update_neighbours_after_destroy(state, level, pos);
    }

    fn get_menu_provider(&self, _state: BlockState, level: &ServerLevel, pos: BlockPos) -> Option<Box<dyn crate::world::inventory::MenuProvider>> {
        BlockEntityMenuProvider::of(level, pos).map(|p| Box::new(p) as Box<dyn crate::world::inventory::MenuProvider>)
    }

    fn get_analog_output_signal(&self, _state: BlockState, level: &ServerLevel, pos: BlockPos, _direction: Direction) -> i32 {
        crate::world::container::get_redstone_signal_from_block_entity(level.get_block_entity(pos))
    }

    fn entity_inside(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        entity: &mut dyn Entity,
        _effect_applier: &mut dyn crate::world::entity::inside_block_effects::InsideBlockEffectApplier,
        _is_precise: bool,
    ) {
        if let Some(item) = entity.as_any_mut().downcast_mut::<ItemEntity>() {
            hopper_block_entity::entity_inside(level, pos, state, item);
        }
    }

    fn is_pathfindable(&self, _state: BlockState, _ty: PathComputationType) -> bool {
        false
    }
}

register_block_class!("HopperBlock", |_| Box::new(HopperBlock));
