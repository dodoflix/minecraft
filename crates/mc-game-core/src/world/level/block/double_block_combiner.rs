//! `net.minecraft.world.level.block.DoubleBlockCombiner`: resolves the one or two block entities (double chests) a block
//! belongs to. Block entities live in their chunk, so the result carries positions; callers look the entities up.

use crate::server::level::ServerLevel;
use crate::world::level::BlockGetter;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::props::EnumProperty;

/// `DoubleBlockCombiner.BlockType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockType {
    Single,
    First,
    Second,
}

/// `DoubleBlockCombiner.NeighborCombineResult`: `acceptNone` / `acceptSingle` / `acceptDouble`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NeighborCombineResult {
    None,
    Single(BlockPos),
    Double { first: BlockPos, second: BlockPos },
}

/// `combineWithNeigbour(entityType, typeResolver, connectionResolver, facingProperty, state, level, pos, blockedChecker)`;
/// `has_entity(pos)` is `entityType.getBlockEntity(level, pos) != null`.
pub fn combine_with_neighbour(
    has_entity: &dyn Fn(&ServerLevel, BlockPos) -> bool,
    type_resolver: fn(BlockState) -> BlockType,
    connection_resolver: fn(BlockState) -> Direction,
    facing_property: EnumProperty<Direction>,
    state: BlockState,
    level: &ServerLevel,
    pos: BlockPos,
    blocked_checker: &dyn Fn(&ServerLevel, BlockPos) -> bool,
) -> NeighborCombineResult {
    if !has_entity(level, pos) || blocked_checker(level, pos) {
        return NeighborCombineResult::None;
    }
    let ty = type_resolver(state);
    if ty == BlockType::Single {
        return NeighborCombineResult::Single(pos);
    }
    let is_first = ty == BlockType::First;
    let neighbor_pos = pos.relative(connection_resolver(state));
    let neighbour_state = level.get_block_state(neighbor_pos);
    if neighbour_state.block() == state.block() {
        let neighbour_type = type_resolver(neighbour_state);
        if neighbour_type != BlockType::Single
            && ty != neighbour_type
            && neighbour_state.get(facing_property) == state.get(facing_property)
        {
            if blocked_checker(level, neighbor_pos) {
                return NeighborCombineResult::None;
            }
            if has_entity(level, neighbor_pos) {
                let (first, second) = if is_first {
                    (pos, neighbor_pos)
                } else {
                    (neighbor_pos, pos)
                };
                return NeighborCombineResult::Double { first, second };
            }
        }
    }
    NeighborCombineResult::Single(pos)
}
