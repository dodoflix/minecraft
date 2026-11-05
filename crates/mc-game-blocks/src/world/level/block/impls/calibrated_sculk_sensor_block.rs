//! `net.minecraft.world.level.block.CalibratedSculkSensorBlock` (extends `SculkSensorBlock`).
use mc_game_core::{register_block_class, update_shape};
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::world::level::block::impls::sculk_sensor_block::{self, SculkSensorBlock};
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::entity::Entity;
use mc_game_core::world::item::BlockPlaceContext;

pub struct CalibratedSculkSensorBlock;

impl BlockBehaviour for CalibratedSculkSensorBlock {
    mc_game_core::simple_waterlogged!();
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        SculkSensorBlock.get_state_for_placement(block, level, context).map(|state| state.set(props::HORIZONTAL_FACING, context.get_horizontal_direction()))
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        SculkSensorBlock.tick(state, level, pos);
    }
    fn step_on(&self, level: &mut ServerLevel, pos: BlockPos, on_state: BlockState, entity: &mut dyn Entity) {
        sculk_sensor_block::step_on_sensor(level, pos, on_state, entity);
    }
    fn on_place(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, old_state: BlockState, moved_by_piston: bool) {
        SculkSensorBlock.on_place(state, level, pos, old_state, moved_by_piston);
    }
    fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, moved_by_piston: bool) {
        SculkSensorBlock.affect_neighbors_after_removal(state, level, pos, moved_by_piston);
    }
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        if state.get(props::WATERLOGGED) {
            helpers::schedule_water_tick(level, pos);
        }
        state
    });
    fn own_signal(&self, state: BlockState, _level: &dyn BlockGetter, _pos: BlockPos) -> i32 {
        state.get(props::POWER)
    }
    /// `getSignal`: not towards the facing direction.
    fn get_signal(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32 {
        if direction != state.get(props::HORIZONTAL_FACING) { self.own_signal(state, level, pos) } else { 0 }
    }
    fn get_direct_signal(&self, state: BlockState, level: &dyn BlockGetter, pos: BlockPos, direction: Direction) -> i32 {
        if direction == Direction::Up { state.get_signal(level, pos, direction) } else { 0 }
    }
    fn get_analog_output_signal(&self, state: BlockState, level: &ServerLevel, pos: BlockPos, _direction: Direction) -> i32 {
        sculk_sensor_block::analog_output_signal(state, level, pos)
    }
    fn spawn_after_break(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, tool: &mc_game_core::world::item::ItemStack, drop_experience: bool) {
        if drop_experience {
            helpers::try_drop_experience(level, pos, tool, &mc_game_core::registry::typed::int_provider::IntProvider::Constant(5));
        }
    }

    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("CalibratedSculkSensorBlock", |_| Box::new(CalibratedSculkSensorBlock));
