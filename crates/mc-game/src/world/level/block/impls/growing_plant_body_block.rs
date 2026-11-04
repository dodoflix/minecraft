//! `net.minecraft.world.level.block.GrowingPlantBodyBlock`.
use super::growing_plant_block::GrowingPlant;
use super::prelude::*;

pub trait GrowingPlantBody: GrowingPlant {
    /// `updateHeadAfterConvertedFromBody(bodyState, headState)`.
    fn update_head_after_converted_from_body(&self, _body_state: BlockState, head_state: BlockState) -> BlockState {
        head_state
    }
}

/// `GrowingPlantBodyBlock.updateShape`.
pub fn update_shape(
    this: &dyn GrowingPlantBody,
    state: BlockState,
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
    direction_to_neighbour: Direction,
    neighbour_state: BlockState,
) -> BlockState {
    let growth = this.growth_direction();
    if direction_to_neighbour == growth.get_opposite() && !state.can_survive(&*level, pos) {
        helpers::schedule_block_tick(level, pos, state.block(), 1);
    }
    let head_block = this.head_block();
    if direction_to_neighbour == growth && !neighbour_state.is_block(state.block()) && !neighbour_state.is_block(head_block) {
        let head_state = super::growing_plant_head_block::get_state_for_placement(head_block, level.random());
        this.update_head_after_converted_from_body(state, head_state)
    } else {
        if this.schedule_fluid_ticks() {
            helpers::schedule_water_tick(level, pos);
        }
        state
    }
}
