//! `net.minecraft.world.level.block.GrowingPlantHeadBlock`.
use super::growing_plant_block::GrowingPlant;
use super::prelude::*;
use mc_core::random::RandomSource;

pub trait GrowingPlantHead: GrowingPlant {
    /// `updateBodyAfterConvertedFromHead(headState, bodyState)`.
    fn update_body_after_converted_from_head(&self, _head_state: BlockState, body_state: BlockState) -> BlockState {
        body_state
    }
}

/// `GrowingPlantHeadBlock.getStateForPlacement(random)`.
pub fn get_state_for_placement(head_block: Block, random: &mut dyn RandomSource) -> BlockState {
    head_block.default_state().set(props::AGE_25, random.next_int_bounded(25))
}

/// `GrowingPlantHeadBlock.updateShape`.
pub fn update_shape(
    this: &dyn GrowingPlantHead,
    state: BlockState,
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
    direction_to_neighbour: Direction,
    neighbour_state: BlockState,
) -> BlockState {
    let growth = this.growth_direction();
    let this_block = state.block();
    if direction_to_neighbour == growth.get_opposite() {
        if !state.can_survive(&*level, pos) {
            helpers::schedule_block_tick(level, pos, this_block, 1);
        } else {
            let neighbor_in_growth_direction = level.get_block_state(pos.relative(growth));
            if neighbor_in_growth_direction.is_block(this_block) || neighbor_in_growth_direction.is_block(this.body_block()) {
                return this.update_body_after_converted_from_head(state, this.body_block().default_state());
            }
        }
    }
    if direction_to_neighbour != growth || (!neighbour_state.is_block(this_block) && !neighbour_state.is_block(this.body_block())) {
        if this.schedule_fluid_ticks() {
            helpers::schedule_water_tick(level, pos);
        }
        state
    } else {
        this.update_body_after_converted_from_head(state, this.body_block().default_state())
    }
}
