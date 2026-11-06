//! `LiquidBlockContainer.placeLiquid` of the vanilla implementors that templates can contain: `SimpleWaterloggedBlock`
//! (every block with a `waterlogged` property, with the `SlabBlock`/`CampfireBlock`/`CandleBlock` overrides) and the
//! kelp/seagrass blocks (which never accept a liquid). Sounds, particles and game events are client/listener side
//! effects that do not exist during world generation.

use crate::world::level::LevelAccessor;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::fluid::{Fluid, FluidState};
use mc_world::ticks::TickPriority;

const NEVER_WATERLOGGED: [&str; 4] = [
    "KelpBlock",
    "KelpPlantBlock",
    "SeagrassBlock",
    "TallSeagrassBlock",
];

fn has_class(state: BlockState, class: &str) -> bool {
    state.block().data().classes.contains(&class)
}

/// `state.getBlock() instanceof LiquidBlockContainer`.
pub fn is_liquid_block_container(state: BlockState) -> bool {
    state.get_property_str("waterlogged").is_some()
        || NEVER_WATERLOGGED.iter().any(|c| has_class(state, c))
}

/// `((LiquidBlockContainer) state.getBlock()).placeLiquid(level, pos, state, fluidState)`.
pub fn place_liquid(
    level: &mut dyn LevelAccessor,
    pos: BlockPos,
    state: BlockState,
    fluid_state: FluidState,
) -> bool {
    if NEVER_WATERLOGGED.iter().any(|c| has_class(state, c)) {
        return false;
    }
    if has_class(state, "SlabBlock") && state.get_property_str("type") == Some("double") {
        return false;
    }
    if state.get_property_str("waterlogged") != Some("false") || fluid_state.fluid() != Fluid::WATER
    {
        return false;
    }
    let waterlogged = state
        .with_property_str("waterlogged", "true")
        .expect("waterlogged property");
    if has_class(state, "CandleBlock") && state.get_property_str("lit") == Some("true") {
        // `extinguish(null, newState, level, pos)`: `setBlock(pos, newState.setValue(LIT, false), 11)`
        level.set_block(
            pos,
            waterlogged
                .with_property_str("lit", "false")
                .expect("lit property"),
            11,
            512,
        );
    } else if has_class(state, "CampfireBlock") {
        level.set_block_and_update(
            pos,
            waterlogged
                .with_property_str("lit", "false")
                .expect("lit property"),
        );
    } else {
        level.set_block_and_update(pos, waterlogged);
    }
    // `scheduleTick(pos, fluidState.getType(), fluidState.getType().getTickDelay(level))`; `WaterFluid` delays 5 ticks
    level.schedule_fluid_tick(pos, Fluid::WATER, 5, TickPriority::Normal);
    true
}
