//! `net.minecraft.world.level.block.LightningRodBlock` (extends `RodBlock`).
use super::prelude::*;
use crate::server::level::ServerLevel;

pub struct LightningRodBlock;

const ACTIVATION_TICKS: i32 = 8;

impl LightningRodBlock {
    /// `onLightningStrike(state, level, pos)`.
    pub fn on_lightning_strike(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        level.set_block_and_update(pos, state.set(props::POWERED, true));
        Self::update_neighbours(state, level, pos);
        helpers::schedule_block_tick(level, pos, state.block(), ACTIVATION_TICKS);
        level.level_event(None, 3002, pos, state.get(props::FACING).get_axis().ordinal() as i32);
    }

    /// `updateNeighbours(state, level, pos)`.
    fn update_neighbours(state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let front = state.get(props::FACING).get_opposite();
        let orientation = level.initial_orientation(Some(front), None);
        level.update_neighbors_at_with_orientation(pos.relative(front), state.block(), orientation);
    }
}

impl BlockBehaviour for LightningRodBlock {
    crate::simple_waterlogged!();
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &crate::world::item::BlockPlaceContext) -> Option<BlockState> {
        let is_water_source = level.get_fluid_state(context.get_clicked_pos()).get_type() == mc_data::fluid::Fluid::WATER;
        Some(block.default_state().set(props::FACING, context.get_clicked_face()).set(props::WATERLOGGED, is_water_source))
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        state.set(props::FACING, mirror.mirror(state.get(props::FACING)))
    }
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        if state.get(props::POWERED) { 15 } else { 0 }
    }
    fn get_direct_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos, direction: Direction) -> i32 {
        if state.get(props::POWERED) && state.get(props::FACING) == direction { 15 } else { 0 }
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        level.set_block_and_update(pos, state.set(props::POWERED, false));
        Self::update_neighbours(state, level, pos);
    }
    fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _moved_by_piston: bool) {
        if state.get(props::POWERED) {
            Self::update_neighbours(state, level, pos);
        }
    }
    fn on_place(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, old_state: BlockState, _moved_by_piston: bool) {
        if state.block() != old_state.block() && state.get(props::POWERED) && !level.has_scheduled_block_tick(pos, state.block()) {
            helpers::schedule_block_tick(level, pos, state.block(), ACTIVATION_TICKS);
        }
    }
}

register_block_class!("LightningRodBlock", |_| Box::new(LightningRodBlock));
