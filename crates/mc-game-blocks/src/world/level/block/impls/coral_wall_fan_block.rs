//! `net.minecraft.world.level.block.CoralWallFanBlock` (extends `BaseCoralWallFanBlock`).
use super::base_coral_plant_type_block::{self as base, dead_block};
use super::base_coral_wall_fan_block as wall_fan;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::{register_block_class, update_shape};

pub struct CoralWallFanBlock {
    dead_block: Block,
}

impl BlockBehaviour for CoralWallFanBlock {
    mc_game_core::simple_waterlogged!();
    fn on_place(
        &self,
        state: BlockState,
        level: &mut ServerLevel,
        pos: BlockPos,
        _old_state: BlockState,
        _moved_by_piston: bool,
    ) {
        base::try_schedule_die_tick(state, level, pos);
    }
    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        if !base::scan_for_water(state, &*level, pos) {
            let dead = self
                .dead_block
                .default_state()
                .set(props::WATERLOGGED, false)
                .set(
                    props::HORIZONTAL_FACING,
                    state.get(props::HORIZONTAL_FACING),
                );
            level.set_block_flags(pos, dead, 2);
        }
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        wall_fan::get_state_for_placement(block, level, context)
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        wall_fan::can_survive(state, level, pos)
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            if direction.get_opposite() == state.get(props::HORIZONTAL_FACING)
                && !state.can_survive(&*level, pos)
            {
                air()
            } else {
                if state.get(props::WATERLOGGED) {
                    helpers::schedule_water_tick(level, pos);
                }
                base::try_schedule_die_tick(state, level, pos);
                wall_fan::update_shape(state, level, pos, direction)
            }
        }
    );
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("CoralWallFanBlock", |block| Box::new(CoralWallFanBlock {
    dead_block: dead_block(block)
}));
