//! `net.minecraft.world.level.block.SideChainPartBlock` and `SideChainPart` (implemented by `ShelfBlock`): powered blocks
//! in a row that connect as left / center / right parts of a chain.
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::block::BlockState;
use mc_data::enums::SideChainPart;
use mc_game_core::world::level::{BlockGetter, LevelAccessor};
use std::collections::{HashMap, VecDeque};

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
fn when_connected_to_the_right(part: SideChainPart) -> SideChainPart {
    match part {
        SideChainPart::Unconnected | SideChainPart::Left => SideChainPart::Left,
        SideChainPart::Right | SideChainPart::Center => SideChainPart::Center,
    }
}

/// `SideChainPart.whenConnectedToTheLeft`.
fn when_connected_to_the_left(part: SideChainPart) -> SideChainPart {
    match part {
        SideChainPart::Unconnected | SideChainPart::Right => SideChainPart::Right,
        SideChainPart::Center | SideChainPart::Left => SideChainPart::Center,
    }
}

/// `SideChainPart.whenDisconnectedFromTheRight`.
fn when_disconnected_from_the_right(part: SideChainPart) -> SideChainPart {
    match part {
        SideChainPart::Unconnected | SideChainPart::Left => SideChainPart::Unconnected,
        SideChainPart::Right | SideChainPart::Center => SideChainPart::Right,
    }
}

/// `SideChainPart.whenDisconnectedFromTheLeft`.
fn when_disconnected_from_the_left(part: SideChainPart) -> SideChainPart {
    match part {
        SideChainPart::Unconnected | SideChainPart::Right => SideChainPart::Unconnected,
        SideChainPart::Center | SideChainPart::Left => SideChainPart::Left,
    }
}

/// `SideChainPartBlock.Neighbor`: the state of one position when it was first asked for (`part: None` = `EmptyNeighbor`).
#[derive(Clone, Copy)]
struct Neighbor {
    pos: BlockPos,
    part: Option<SideChainPart>,
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
            .is_some_and(|part| is_connection_towards(part, end_part))
    }
}

/// `SideChainPartBlock.Neighbors`: the per-call cache of neighbours, created lazily like vanilla's (a neighbour is read
/// the first time it is asked for, even when an earlier update already changed other positions).
struct Neighbors {
    facing: Direction,
    center: BlockPos,
    cache: HashMap<BlockPos, Neighbor>,
}

impl Neighbors {
    fn new(facing: Direction, center: BlockPos) -> Self {
        Self {
            facing,
            center,
            cache: HashMap::new(),
        }
    }

    fn get_or_create(
        &mut self,
        block: &(impl SideChainPartBlock + ?Sized),
        level: &dyn BlockGetter,
        dir: Direction,
        steps: i32,
    ) -> Neighbor {
        let pos = self.center.relative_n(dir, steps);
        let facing = self.facing;
        *self.cache.entry(pos).or_insert_with(|| {
            let state = level.get_block_state(pos);
            let connectable = block.is_connectable(state) && block.get_facing(state) == facing;
            Neighbor {
                pos,
                part: connectable.then(|| block.get_side_chain_part(state)),
            }
        })
    }

    fn left_n(
        &mut self,
        block: &(impl SideChainPartBlock + ?Sized),
        level: &dyn BlockGetter,
        steps: i32,
    ) -> Neighbor {
        self.get_or_create(block, level, self.facing.get_clockwise(), steps)
    }

    fn right_n(
        &mut self,
        block: &(impl SideChainPartBlock + ?Sized),
        level: &dyn BlockGetter,
        steps: i32,
    ) -> Neighbor {
        self.get_or_create(block, level, self.facing.get_counter_clockwise(), steps)
    }
}

pub trait SideChainPartBlock {
    fn get_side_chain_part(&self, state: BlockState) -> SideChainPart;
    fn set_side_chain_part(&self, state: BlockState, new_part: SideChainPart) -> BlockState;
    fn get_facing(&self, state: BlockState) -> Direction;
    fn is_connectable(&self, state: BlockState) -> bool;
    fn get_max_chain_length(&self) -> i32;

    /// `getAllBlocksConnectedTo(level, pos)`: the chain `pos` belongs to, left to right (empty when `pos` is not
    /// connectable).
    fn get_all_blocks_connected_to(&self, level: &dyn BlockGetter, pos: BlockPos) -> Vec<BlockPos> {
        let state = level.get_block_state(pos);
        if !self.is_connectable(state) {
            return Vec::new();
        }
        let mut neighbors = Neighbors::new(self.get_facing(state), pos);
        let mut results = VecDeque::from([pos]);
        for steps in 1..self.get_max_chain_length() {
            let neighbor = neighbors.left_n(self, level, steps);
            if neighbor.connects_towards(SideChainPart::Left) {
                results.push_front(neighbor.pos);
            }
            if neighbor.is_unconnectable_or_chain_end() {
                break;
            }
        }
        for steps in 1..self.get_max_chain_length() {
            let neighbor = neighbors.right_n(self, level, steps);
            if neighbor.connects_towards(SideChainPart::Right) {
                results.push_back(neighbor.pos);
            }
            if neighbor.is_unconnectable_or_chain_end() {
                break;
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
        let mut neighbors = Neighbors::new(self.get_facing(state), pos);
        let left = neighbors.left_n(self, &*level, 1);
        if let Some(part) = left.part {
            self.set_part(level, left.pos, when_disconnected_from_the_right(part));
        }
        let right = neighbors.right_n(self, &*level, 1);
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
        // `isBeingUpdatedByNeighbor`
        let is_getting_connected = is_connected(self.get_side_chain_part(state));
        let has_been_connected_before =
            self.is_connectable(old_state) && is_connected(self.get_side_chain_part(old_state));
        if is_getting_connected || has_been_connected_before {
            return;
        }
        let mut neighbors = Neighbors::new(self.get_facing(state), pos);
        let mut new_part_for_self = SideChainPart::Unconnected;
        let left = neighbors.left_n(self, &*level, 1);
        let existing_chain_on_the_left = if left.is_connectable() {
            self.get_all_blocks_connected_to(&*level, left.pos).len() as i32
        } else {
            0
        };
        let right = neighbors.right_n(self, &*level, 1);
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

    /// `canConnect(newBlocksToConnectTo, currentChainLength)`.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_transitions_match_vanilla() {
        use SideChainPart::{Center, Left, Right, Unconnected};
        assert_eq!(when_connected_to_the_right(Unconnected), Left);
        assert_eq!(when_connected_to_the_right(Right), Center);
        assert_eq!(when_connected_to_the_left(Unconnected), Right);
        assert_eq!(when_connected_to_the_left(Left), Center);
        assert_eq!(when_disconnected_from_the_right(Center), Right);
        assert_eq!(when_disconnected_from_the_right(Left), Unconnected);
        assert_eq!(when_disconnected_from_the_left(Center), Left);
        assert_eq!(when_disconnected_from_the_left(Right), Unconnected);
    }
}
