//! `net.minecraft.world.level.block.CreakingHeartBlock`.
use super::prelude::*;
use super::rotated_pillar_block::rotate_pillar;
use crate::server::level::ServerLevel;
use crate::world::block_entity::CreakingHeartBlockEntity;
use crate::world::item::BlockPlaceContext;
use mc_data::enums::CreakingHeartState;

pub struct CreakingHeartBlock;

/// `hasRequiredLogs(state, level, pos)`.
pub fn has_required_logs(state: BlockState, level: &dyn BlockGetter, pos: BlockPos) -> bool {
    let axis = state.get(props::AXIS);
    Direction::VALUES.into_iter().filter(|d| d.get_axis() == axis).all(|dir| {
        let neighbour = level.get_block_state(pos.relative(dir));
        neighbour.is_tag("pale_oak_logs") && neighbour.get(props::AXIS) == axis
    })
}

/// `updateState(state, level, pos)`.
fn update_state(state: BlockState, level: &ServerLevel, pos: BlockPos) -> BlockState {
    let has_logs = has_required_logs(state, level, pos);
    let disabled = state.get(props::CREAKING_HEART_STATE) == CreakingHeartState::Uprooted;
    if has_logs && disabled {
        state.set(
            props::CREAKING_HEART_STATE,
            if level.attribute_bool(crate::world::attribute::attributes::CREAKING_ACTIVE, pos) { CreakingHeartState::Awake } else { CreakingHeartState::Dormant },
        )
    } else {
        state
    }
}

impl BlockBehaviour for CreakingHeartBlock {
    fn get_state_for_placement(&self, block: Block, level: &mut ServerLevel, context: &BlockPlaceContext) -> Option<BlockState> {
        Some(update_state(block.default_state().set(props::AXIS, context.get_clicked_face().get_axis()), level, context.get_clicked_pos()))
    }

    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        helpers::schedule_block_tick(level, pos, state.block(), 1);
        state
    });

    fn tick(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos) {
        let new_state = update_state(state, level, pos);
        if new_state != state {
            level.set_block_and_update(pos, new_state);
        }
    }

    fn rotate(&self, state: BlockState, rotation: Rotation) -> BlockState {
        rotate_pillar(state, rotation)
    }

    /// `Containers.updateNeighboursAfterDestroy(state, level, pos)`.
    fn affect_neighbors_after_removal(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, _moved_by_piston: bool) {
        level.update_neighbour_for_output_signal(pos, state.block());
    }

    fn get_analog_output_signal(&self, state: BlockState, level: &ServerLevel, pos: BlockPos, _direction: Direction) -> i32 {
        if state.get(props::CREAKING_HEART_STATE) == CreakingHeartState::Uprooted {
            0
        } else {
            level.get_block_entity(pos).and_then(|be| be.as_any().downcast_ref::<CreakingHeartBlockEntity>()).map_or(0, CreakingHeartBlockEntity::get_analog_output_signal)
        }
    }
}

register_block_class!("CreakingHeartBlock", |_| Box::new(CreakingHeartBlock));
