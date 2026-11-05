//! `net.minecraft.world.level.redstone.RedstoneWireEvaluator` (the wire block is always `Block::REDSTONE_WIRE`).
use super::Orientation;
use crate::server::level::ServerLevel;
use crate::world::level::BlockGetter;
use crate::world::level::block::BlockStateExt;
use crate::world::level::block::impls::redstone_wire_block;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::props;

pub trait RedstoneWireEvaluator {
    /// `updatePowerStrength(level, pos, state, orientation, skipShapeUpdates)`.
    fn update_power_strength(&mut self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, orientation: Option<Orientation>, skip_shape_updates: bool);

    /// `getBlockSignal(level, pos)`.
    fn get_block_signal(&self, level: &mut ServerLevel, pos: BlockPos) -> i32 {
        redstone_wire_block::get_block_signal(level, pos)
    }

    /// `getWireSignal(pos, state)`.
    fn get_wire_signal(&self, _pos: BlockPos, state: BlockState) -> i32 {
        if state.is_block(Block::REDSTONE_WIRE) { state.get(props::POWER) } else { 0 }
    }

    /// `getIncomingWireSignal(level, pos)`.
    fn get_incoming_wire_signal(&self, level: &dyn BlockGetter, pos: BlockPos) -> i32 {
        let mut wire_signal = 0;
        for direction in Direction::HORIZONTAL {
            let neighbor_pos = pos.relative(direction);
            let neighbor_state = level.get_block_state(neighbor_pos);
            wire_signal = wire_signal.max(self.get_wire_signal(neighbor_pos, neighbor_state));
            let above_pos = pos.above();
            if neighbor_state.is_redstone_conductor() && !level.get_block_state(above_pos).is_redstone_conductor() {
                let above_neighbor_pos = neighbor_pos.above();
                wire_signal = wire_signal.max(self.get_wire_signal(above_neighbor_pos, level.get_block_state(above_neighbor_pos)));
            } else if !neighbor_state.is_redstone_conductor() {
                let below_neighbor_pos = neighbor_pos.below();
                wire_signal = wire_signal.max(self.get_wire_signal(below_neighbor_pos, level.get_block_state(below_neighbor_pos)));
            }
        }
        (wire_signal - 1).max(0)
    }
}
