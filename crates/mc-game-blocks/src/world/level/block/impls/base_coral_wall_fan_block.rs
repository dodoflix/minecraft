//! `net.minecraft.world.level.block.BaseCoralWallFanBlock` (extends `BaseCoralFanBlock`).
use super::base_coral_plant_type_block as base;
use mc_game_core::server::level::ServerLevel;
use mc_game_core::world::item::BlockPlaceContext;
use mc_game_core::world::level::block::impls::prelude::*;
use mc_game_core::{register_block_class, update_shape};

pub struct BaseCoralWallFanBlock;

/// `BaseCoralWallFanBlock.canSurvive`.
pub fn can_survive(state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
    let facing = state.get(props::HORIZONTAL_FACING);
    let relative_pos = pos.relative(facing.get_opposite());
    level
        .get_block_state(relative_pos)
        .is_face_sturdy_at(level, relative_pos, facing)
}

/// `BaseCoralWallFanBlock.updateShape`.
pub fn update_shape(
    state: BlockState,
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
    direction_to_neighbour: Direction,
) -> BlockState {
    if state.get(props::WATERLOGGED) {
        helpers::schedule_water_tick(level, pos);
    }
    if direction_to_neighbour.get_opposite() == state.get(props::HORIZONTAL_FACING)
        && !state.can_survive(&*level, pos)
    {
        air()
    } else {
        state
    }
}

/// `BaseCoralWallFanBlock.getStateForPlacement`.
pub fn get_state_for_placement(
    block: Block,
    level: &mut ServerLevel,
    context: &BlockPlaceContext,
) -> Option<BlockState> {
    let mut state = base::get_state_for_placement(block, level, context);
    let pos = context.get_clicked_pos();
    for direction in context.get_nearest_looking_directions() {
        if direction.get_axis().is_horizontal() {
            state = state.set(props::HORIZONTAL_FACING, direction.get_opposite());
            if state.can_survive(&*level, pos) {
                return Some(state);
            }
        }
    }
    None
}

impl BlockBehaviour for BaseCoralWallFanBlock {
    mc_game_core::simple_waterlogged!();
    fn get_state_for_placement(
        &self,
        block: Block,
        level: &mut ServerLevel,
        context: &BlockPlaceContext,
    ) -> Option<BlockState> {
        get_state_for_placement(block, level, context)
    }
    fn can_survive(&self, state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> bool {
        can_survive(state, level, pos)
    }
    update_shape!(
        |self, state, level, pos, direction, neighbour_pos, neighbour_state| {
            update_shape(state, level, pos, direction)
        }
    );
    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        helpers::rotate_facing(state, props::HORIZONTAL_FACING, rotation)
    }
    fn mirror(&self, state: BlockState, mirror: Mirror) -> BlockState {
        helpers::mirror_facing(state, props::HORIZONTAL_FACING, mirror)
    }
}

register_block_class!("BaseCoralWallFanBlock", |_| Box::new(BaseCoralWallFanBlock));
