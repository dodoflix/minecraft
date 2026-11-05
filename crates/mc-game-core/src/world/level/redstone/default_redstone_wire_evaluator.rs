//! `net.minecraft.world.level.redstone.DefaultRedstoneWireEvaluator`.
use super::Orientation;
use super::redstone_wire_evaluator::RedstoneWireEvaluator;
use crate::server::level::ServerLevel;
use crate::world::level::levelgen::feature::tree::java_hash_set::BlockPosHashSet;
use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};
use mc_data::props;

pub struct DefaultRedstoneWireEvaluator;

impl DefaultRedstoneWireEvaluator {
    fn calculate_target_strength(&self, level: &mut ServerLevel, pos: BlockPos) -> i32 {
        let block_signal = self.get_block_signal(level, pos);
        if block_signal == 15 { block_signal } else { block_signal.max(self.get_incoming_wire_signal(level, pos)) }
    }
}

impl RedstoneWireEvaluator for DefaultRedstoneWireEvaluator {
    fn update_power_strength(&mut self, level: &mut ServerLevel, pos: BlockPos, state: BlockState, _orientation: Option<Orientation>, _skip_shape_updates: bool) {
        let target_strength = self.calculate_target_strength(level, pos);
        if state.get(props::POWER) != target_strength {
            if level.get_block_state(pos) == state {
                level.set_block_flags(pos, state.set(props::POWER, target_strength), 2);
            }
            // `Sets.newHashSet()`: iteration in `java.util.HashSet<BlockPos>` order
            let mut to_update = BlockPosHashSet::new();
            to_update.insert(pos);
            for direction in Direction::ALL {
                to_update.insert(pos.relative(direction));
            }
            for block_pos in to_update.iter().collect::<Vec<_>>() {
                level.update_neighbors_at(block_pos, Block::REDSTONE_WIRE);
            }
        }
    }
}
