//! `net.minecraft.world.level.block.RailBlock` (extends `BaseRailBlock`).
use super::base_rail_block::{self, rail_behaviour};
use super::prelude::*;
use crate::server::level::ServerLevel;
use crate::world::level::block::rail_state::RailState;

pub struct RailBlock;

/// `RailBlock.updateState(state, level, pos, block)`.
fn update_state(state: BlockState, level: &mut ServerLevel, pos: BlockPos, block: Block) {
    if block.default_state().is_signal_source() && RailState::new(pos, state).count_potential_connections(&*level) == 3 {
        base_rail_block::update_dir(level, pos, state, false);
    }
}



rail_behaviour!(RailBlock, update_state, base_rail_block::on_place);

register_block_class!("RailBlock", |_| Box::new(RailBlock));
