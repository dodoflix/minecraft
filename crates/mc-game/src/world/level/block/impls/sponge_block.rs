//! `net.minecraft.world.level.block.SpongeBlock`.
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::level::block::behaviour;
use crate::world::level::material::FluidStateExt;
use mc_core::pos::TraversalNodeStatus;
use mc_data::SoundEvent;
use mc_protocol::packets::play::level::SoundSource;

pub struct SpongeBlock;

const MAX_DEPTH: i32 = 6;
const MAX_COUNT: i32 = 65;

/// `tryAbsorbWater`.
fn try_absorb_water(level: &mut ServerLevel, pos: BlockPos) {
    if remove_water_breadth_first_search(level, pos) {
        level.set_block_flags(pos, Block::WET_SPONGE.default_state(), 2);
        level.play_sound(None, pos, SoundEvent::BLOCK_SPONGE_ABSORB, SoundSource::Blocks, 1.0, 1.0);
    }
}

/// `removeWaterBreadthFirstSearch`.
fn remove_water_breadth_first_search(level: &mut ServerLevel, start_pos: BlockPos) -> bool {
    BlockPos::breadth_first_traversal(
        start_pos,
        MAX_DEPTH,
        MAX_COUNT,
        |pos, consumer| {
            for direction in Direction::ALL {
                consumer(pos.relative(direction));
            }
        },
        |pos| {
            if pos == start_pos {
                return TraversalNodeStatus::Accept;
            }
            let state = level.get_block_state(pos);
            let fluid_state = level.get_fluid_state(pos);
            if !fluid_state.is_tag("water") {
                TraversalNodeStatus::Skip
            } else if behaviour(state.block()).is_bucket_pickup() && !behaviour(state.block()).pickup_block(None, level, pos, state).is_empty() {
                TraversalNodeStatus::Accept
            } else if helpers::is_instance_of(state, "LiquidBlock") {
                level.set_block_and_update(pos, air());
                TraversalNodeStatus::Accept
            } else if !state.is_block(Block::KELP) && !state.is_block(Block::KELP_PLANT) && !state.is_block(Block::SEAGRASS) && !state.is_block(Block::TALL_SEAGRASS) {
                TraversalNodeStatus::Skip
            } else {
                helpers::drop_resources(level, state, pos, None);
                level.set_block_and_update(pos, air());
                TraversalNodeStatus::Accept
            }
        },
    ) > 1
}

impl BlockBehaviour for SpongeBlock {
    fn on_place(&self, state: BlockState, level: &mut ServerLevel, pos: BlockPos, old_state: BlockState, _moved_by_piston: bool) {
        if !old_state.is_block(state.block()) {
            try_absorb_water(level, pos);
        }
    }
    fn neighbor_changed(&self, _state: BlockState, level: &mut ServerLevel, pos: BlockPos, _block: Block, _orientation: Option<crate::world::level::redstone::Orientation>, _moved_by_piston: bool) {
        try_absorb_water(level, pos);
    }
}

register_block_class!("SpongeBlock", |_| Box::new(SpongeBlock));
