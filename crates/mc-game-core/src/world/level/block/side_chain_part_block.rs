//! `SideChainPartBlock` (implemented by `ShelfBlock`): a row of blocks that connect left/center/right when powered.
//! Vanilla's per-call `Neighbors` cache is not needed: every position is read before it is modified.

use crate::world::level::{BlockGetter, LevelAccessor};
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::SideChainPart;
use std::collections::VecDeque;

/// `SideChainPart.isConnected`.
fn is_connected(part: SideChainPart) -> bool {
    part != SideChainPart::Unconnected
}

/// `SideChainPart.isConnectionTowards`.
fn is_connection_towards(part: SideChainPart, end_part: SideChainPart) -> bool {
    part == SideChainPart::Center || part == end_part
}

/// `SideChainPart.isChainEnd`.
fn is_chain_end(part: SideChainPart) -> bool {
    part != SideChainPart::Center
}

/// `SideChainPart.whenConnectedToTheRight`.
pub fn when_connected_to_the_right(part: SideChainPart) -> SideChainPart {
    match part {
        SideChainPart::Unconnected | SideChainPart::Left => SideChainPart::Left,
        SideChainPart::Right | SideChainPart::Center => SideChainPart::Center,
    }
}

/// `SideChainPart.whenConnectedToTheLeft`.
pub fn when_connected_to_the_left(part: SideChainPart) -> SideChainPart {
    match part {
        SideChainPart::Unconnected | SideChainPart::Right => SideChainPart::Right,
        SideChainPart::Center | SideChainPart::Left => SideChainPart::Center,
    }
}

/// `SideChainPart.whenDisconnectedFromTheRight`.
pub fn when_disconnected_from_the_right(part: SideChainPart) -> SideChainPart {
    match part {
        SideChainPart::Unconnected | SideChainPart::Left => SideChainPart::Unconnected,
        SideChainPart::Right | SideChainPart::Center => SideChainPart::Right,
    }
}

/// `SideChainPart.whenDisconnectedFromTheLeft`.
pub fn when_disconnected_from_the_left(part: SideChainPart) -> SideChainPart {
    match part {
        SideChainPart::Unconnected | SideChainPart::Right => SideChainPart::Unconnected,
        SideChainPart::Center | SideChainPart::Left => SideChainPart::Left,
    }
}

/// `SideChainPartBlock.Neighbor` (`EmptyNeighbor` = `part: None`).
#[derive(Clone, Copy)]
pub struct Neighbor {
    pub pos: BlockPos,
    pub part: Option<SideChainPart>,
}

impl Neighbor {
    fn is_connectable(&self) -> bool {
        self.part.is_some()
    }
    fn is_unconnectable_or_chain_end(&self) -> bool {
        self.part.is_none_or(is_chain_end)
    }
    fn connects_towards(&self, end_part: SideChainPart) -> bool {
        self.part
            .is_some_and(|p| is_connection_towards(p, end_part))
    }
}

pub trait SideChainPartBlock {
    fn get_side_chain_part(&self, state: BlockState) -> SideChainPart;
    fn set_side_chain_part(&self, state: BlockState, new_part: SideChainPart) -> BlockState;
    fn get_facing(&self, state: BlockState) -> Direction;
    fn is_connectable(&self, state: BlockState) -> bool;
    fn get_max_chain_length(&self) -> i32;

    /// `Neighbors.left(steps)` (`right` = counter-clockwise) built from the current level state.
    fn neighbor(
        &self,
        level: &dyn BlockGetter,
        facing: Direction,
        center: BlockPos,
        left: bool,
        steps: i32,
    ) -> Neighbor {
        let dir = if left {
            facing.get_clockwise()
        } else {
            facing.get_counter_clockwise()
        };
        let pos = center.relative_n(dir, steps);
        let state = level.get_block_state(pos);
        let part = if self.is_connectable(state) && self.get_facing(state) == facing {
            Some(self.get_side_chain_part(state))
        } else {
            None
        };
        Neighbor { pos, part }
    }

    /// `getAllBlocksConnectedTo(level, pos)`.
    fn get_all_blocks_connected_to(&self, level: &dyn BlockGetter, pos: BlockPos) -> Vec<BlockPos> {
        let state = level.get_block_state(pos);
        if !self.is_connectable(state) {
            return Vec::new();
        }
        let facing = self.get_facing(state);
        let mut results: VecDeque<BlockPos> = VecDeque::new();
        results.push_back(pos);
        for (left, end_part) in [(true, SideChainPart::Left), (false, SideChainPart::Right)] {
            for steps in 1..self.get_max_chain_length() {
                let neighbor = self.neighbor(level, facing, pos, left, steps);
                if neighbor.connects_towards(end_part) {
                    if left {
                        results.push_front(neighbor.pos)
                    } else {
                        results.push_back(neighbor.pos)
                    }
                }
                if neighbor.is_unconnectable_or_chain_end() {
                    break;
                }
            }
        }
        results.into()
    }

    /// `updateNeighborsAfterPoweringDown(level, pos, state)`.
    fn update_neighbors_after_powering_down(
        &self,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        state: BlockState,
    ) {
        let facing = self.get_facing(state);
        let left = self.neighbor(&*level, facing, pos, true, 1);
        let right = self.neighbor(&*level, facing, pos, false, 1);
        if let Some(part) = left.part {
            self.set_part(level, left.pos, when_disconnected_from_the_right(part));
        }
        if let Some(part) = right.part {
            self.set_part(level, right.pos, when_disconnected_from_the_left(part));
        }
    }

    /// `updateSelfAndNeighborsOnPoweringUp(level, pos, state, oldState)`.
    fn update_self_and_neighbors_on_powering_up(
        &self,
        level: &mut dyn LevelAccessor,
        pos: BlockPos,
        state: BlockState,
        old_state: BlockState,
    ) {
        if !self.is_connectable(state) {
            return;
        }
        let is_getting_connected = is_connected(self.get_side_chain_part(state));
        let has_been_connected_before =
            self.is_connectable(old_state) && is_connected(self.get_side_chain_part(old_state));
        if is_getting_connected || has_been_connected_before {
            return;
        }
        let facing = self.get_facing(state);
        let left = self.neighbor(&*level, facing, pos, true, 1);
        let right = self.neighbor(&*level, facing, pos, false, 1);
        let mut new_part_for_self = SideChainPart::Unconnected;
        let existing_chain_on_the_left = if left.is_connectable() {
            self.get_all_blocks_connected_to(&*level, left.pos).len() as i32
        } else {
            0
        };
        let existing_chain_on_the_right = if right.is_connectable() {
            self.get_all_blocks_connected_to(&*level, right.pos).len() as i32
        } else {
            0
        };
        let mut current_chain_length = 1;
        if self.can_connect(existing_chain_on_the_left, current_chain_length) {
            new_part_for_self = when_connected_to_the_left(new_part_for_self);
            if let Some(part) = left.part {
                self.set_part(level, left.pos, when_connected_to_the_right(part));
            }
            current_chain_length += existing_chain_on_the_left;
        }
        if self.can_connect(existing_chain_on_the_right, current_chain_length) {
            new_part_for_self = when_connected_to_the_right(new_part_for_self);
            if let Some(part) = right.part {
                self.set_part(level, right.pos, when_connected_to_the_left(part));
            }
        }
        self.set_part(level, pos, new_part_for_self);
    }

    fn can_connect(&self, new_blocks_to_connect_to: i32, current_chain_length: i32) -> bool {
        new_blocks_to_connect_to > 0
            && current_chain_length + new_blocks_to_connect_to <= self.get_max_chain_length()
    }

    /// `setPart(level, pos, newPart)`.
    fn set_part(&self, level: &mut dyn LevelAccessor, pos: BlockPos, new_part: SideChainPart) {
        let state = level.get_block_state(pos);
        if self.get_side_chain_part(state) != new_part {
            level.set_block_and_update(pos, self.set_side_chain_part(state, new_part));
        }
    }
}
