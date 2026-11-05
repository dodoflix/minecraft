//! `net.minecraft.world.level.block.PotentSulfurBlock` (extends `BaseEntityBlock`).
use super::prelude::*;
use mc_data::enums::PotentSulfurState;

pub struct PotentSulfurBlock;

fn is_source_if_fluid(below_state: BlockState) -> bool {
    let fluid = below_state.get_fluid_state();
    fluid.is_empty() || fluid.is_source()
}

/// `PotentSulfurBlock.validBlockState` (the `resetCountdown` of the block entity is not ported yet).
fn valid_block_state(state: BlockState, level: &dyn LevelReader, pos: BlockPos) -> BlockState {
    if !level.get_fluid_state(pos.above()).is_source_of_type(mc_data::fluid::Fluid::WATER) {
        return state.set(props::POTENT_SULFUR_STATE, PotentSulfurState::Dry);
    }
    let below_state = level.get_block_state(pos.below());
    if below_state.is_tag("causes_continuous_geyser_eruptions") && is_source_if_fluid(below_state) {
        state.set(props::POTENT_SULFUR_STATE, PotentSulfurState::Continuous)
    } else if below_state.is_tag("causes_periodic_geyser_eruptions") && is_source_if_fluid(below_state) {
        if state.get(props::POTENT_SULFUR_STATE) == PotentSulfurState::Erupting { state } else { state.set(props::POTENT_SULFUR_STATE, PotentSulfurState::Dormant) }
    } else {
        state.set(props::POTENT_SULFUR_STATE, PotentSulfurState::Wet)
    }
}

impl BlockBehaviour for PotentSulfurBlock {
    update_shape!(|self, state, level, pos, direction, neighbour_pos, neighbour_state| {
        valid_block_state(state, &*level, pos)
    });
}

register_block_class!("PotentSulfurBlock", |_| Box::new(PotentSulfurBlock));
