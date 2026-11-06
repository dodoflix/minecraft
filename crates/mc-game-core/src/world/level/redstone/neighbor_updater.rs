//! `NeighborUpdater` / `CollectingNeighborUpdater`: neighbor and shape updates are queued and executed depth-first
//! in a deterministic order, with a cap on chained updates (`max-chained-neighbor-updates`).
//!
//! The updater *state* lives inside the level; the level is passed to every call as the [`NeighborUpdateHost`]
//! (vanilla's updater holds a back reference to its `Level`, which Rust ownership forbids).

use super::orientation::Orientation;
use mc_core::direction::Direction;
use mc_core::pos::BlockPos;
use mc_data::block::{Block, BlockState};

/// `NeighborUpdater.UPDATE_ORDER`.
pub const UPDATE_ORDER: [Direction; 6] = [
    Direction::West,
    Direction::East,
    Direction::Down,
    Direction::Up,
    Direction::North,
    Direction::South,
];

/// What the updater needs from its level.
pub trait NeighborUpdateHost {
    fn neighbor_updater(&mut self) -> &mut CollectingNeighborUpdater;
    fn host_block_state(&self, pos: BlockPos) -> BlockState;
    /// `NeighborUpdater.executeShapeUpdate`.
    fn execute_shape_update(
        &mut self,
        direction: Direction,
        pos: BlockPos,
        neighbor_pos: BlockPos,
        neighbor_state: BlockState,
        update_flags: u32,
        update_limit: i32,
    );
    /// `NeighborUpdater.executeUpdate` (`state.handleNeighborChanged`).
    fn execute_update(
        &mut self,
        state: BlockState,
        pos: BlockPos,
        changed_block: Block,
        orientation: Option<Orientation>,
        moved_by_piston: bool,
    );
    /// `level.enabledFeatures().contains(FeatureFlags.REDSTONE_EXPERIMENTS)`.
    fn redstone_experiments(&self) -> bool;
    /// `ExperimentalRedstoneUtils.initialOrientation(level, front, up)` (`None` unless the feature is enabled;
    /// consumes one `nextInt(48)` of the level RNG otherwise).
    fn initial_orientation(
        &mut self,
        front: Option<Direction>,
        up: Option<Direction>,
    ) -> Option<Orientation>;
}

/// One queued unit of work (`CollectingNeighborUpdater.NeighborUpdates`).
#[derive(Clone, Debug, PartialEq)]
pub enum NeighborUpdates {
    Shape {
        direction: Direction,
        neighbor_state: BlockState,
        pos: BlockPos,
        neighbor_pos: BlockPos,
        update_flags: u32,
        update_limit: i32,
    },
    Simple {
        pos: BlockPos,
        block: Block,
        orientation: Option<Orientation>,
    },
    Full {
        state: BlockState,
        pos: BlockPos,
        block: Block,
        orientation: Option<Orientation>,
        moved_by_piston: bool,
    },
    Multi {
        source_pos: BlockPos,
        source_block: Block,
        orientation: Option<Orientation>,
        skip_direction: Option<Direction>,
        idx: usize,
    },
}

impl NeighborUpdates {
    /// `new MultiNeighborUpdate(..)` (skips the first direction when it is the skipped one).
    fn multi(
        source_pos: BlockPos,
        source_block: Block,
        orientation: Option<Orientation>,
        skip: Option<Direction>,
    ) -> Self {
        let idx = usize::from(Some(UPDATE_ORDER[0]) == skip);
        NeighborUpdates::Multi {
            source_pos,
            source_block,
            orientation,
            skip_direction: skip,
            idx,
        }
    }

    /// `runNext(level)`: returns whether more work remains.
    fn run_next<H: NeighborUpdateHost>(&mut self, host: &mut H) -> bool {
        match self {
            NeighborUpdates::Shape {
                direction,
                neighbor_state,
                pos,
                neighbor_pos,
                update_flags,
                update_limit,
            } => {
                host.execute_shape_update(
                    *direction,
                    *pos,
                    *neighbor_pos,
                    *neighbor_state,
                    *update_flags,
                    *update_limit,
                );
                false
            }
            NeighborUpdates::Simple {
                pos,
                block,
                orientation,
            } => {
                let state = host.host_block_state(*pos);
                host.execute_update(state, *pos, *block, *orientation, false);
                false
            }
            NeighborUpdates::Full {
                state,
                pos,
                block,
                orientation,
                moved_by_piston,
            } => {
                host.execute_update(*state, *pos, *block, *orientation, *moved_by_piston);
                false
            }
            NeighborUpdates::Multi {
                source_pos,
                source_block,
                orientation,
                skip_direction,
                idx,
            } => {
                let direction = UPDATE_ORDER[*idx];
                *idx += 1;
                let neighbor_pos = source_pos.relative(direction);
                let state = host.host_block_state(neighbor_pos);
                let mut neighbor_orientation = None;
                if host.redstone_experiments() {
                    if orientation.is_none() {
                        *orientation = host
                            .initial_orientation(skip_direction.map(Direction::get_opposite), None);
                    }
                    neighbor_orientation = orientation.map(|o| o.with_front(direction));
                }
                host.execute_update(
                    state,
                    neighbor_pos,
                    *source_block,
                    neighbor_orientation,
                    false,
                );
                if *idx < UPDATE_ORDER.len() && Some(UPDATE_ORDER[*idx]) == *skip_direction {
                    *idx += 1;
                }
                *idx < UPDATE_ORDER.len()
            }
        }
    }

    /// `forEachUpdatedPos`.
    pub fn for_each_updated_pos(&self, mut output: impl FnMut(BlockPos)) {
        match self {
            NeighborUpdates::Shape { pos, .. }
            | NeighborUpdates::Simple { pos, .. }
            | NeighborUpdates::Full { pos, .. } => output(*pos),
            NeighborUpdates::Multi {
                source_pos,
                skip_direction,
                ..
            } => {
                for direction in UPDATE_ORDER {
                    if Some(direction) != *skip_direction {
                        output(source_pos.relative(direction));
                    }
                }
            }
        }
    }
}

/// `CollectingNeighborUpdater` state.
pub struct CollectingNeighborUpdater {
    max_chained_neighbor_updates: i32,
    /// Top of the stack is the last element (vanilla `ArrayDeque.push` = `addFirst`, `peek` = `peekFirst`).
    stack: Vec<NeighborUpdates>,
    added_this_layer: Vec<NeighborUpdates>,
    count: i32,
}

impl CollectingNeighborUpdater {
    pub fn new(max_chained_neighbor_updates: i32) -> Self {
        Self {
            max_chained_neighbor_updates,
            stack: Vec::new(),
            added_this_layer: Vec::new(),
            count: 0,
        }
    }

    pub fn shape_update<H: NeighborUpdateHost>(
        host: &mut H,
        direction: Direction,
        neighbor_state: BlockState,
        pos: BlockPos,
        neighbor_pos: BlockPos,
        update_flags: u32,
        update_limit: i32,
    ) {
        Self::add_and_run(
            host,
            pos,
            NeighborUpdates::Shape {
                direction,
                neighbor_state,
                pos,
                neighbor_pos,
                update_flags,
                update_limit,
            },
        );
    }

    pub fn neighbor_changed<H: NeighborUpdateHost>(
        host: &mut H,
        pos: BlockPos,
        block: Block,
        orientation: Option<Orientation>,
    ) {
        Self::add_and_run(
            host,
            pos,
            NeighborUpdates::Simple {
                pos,
                block,
                orientation,
            },
        );
    }

    pub fn neighbor_changed_with_state<H: NeighborUpdateHost>(
        host: &mut H,
        state: BlockState,
        pos: BlockPos,
        block: Block,
        orientation: Option<Orientation>,
        moved_by_piston: bool,
    ) {
        Self::add_and_run(
            host,
            pos,
            NeighborUpdates::Full {
                state,
                pos,
                block,
                orientation,
                moved_by_piston,
            },
        );
    }

    pub fn update_neighbors_at_except_from_facing<H: NeighborUpdateHost>(
        host: &mut H,
        pos: BlockPos,
        block: Block,
        skip_direction: Option<Direction>,
        orientation: Option<Orientation>,
    ) {
        Self::add_and_run(
            host,
            pos,
            NeighborUpdates::multi(pos, block, orientation, skip_direction),
        );
    }

    /// Number of updates accepted or rejected since the outermost call started (0 when idle).
    pub fn count(&self) -> i32 {
        self.count
    }

    fn add_and_run<H: NeighborUpdateHost>(host: &mut H, pos: BlockPos, update: NeighborUpdates) {
        let this = host.neighbor_updater();
        let running_already = this.count > 0;
        let too_many = this.max_chained_neighbor_updates >= 0
            && this.count >= this.max_chained_neighbor_updates;
        this.count += 1;
        if !too_many {
            if running_already {
                this.added_this_layer.push(update);
            } else {
                this.stack.push(update);
            }
        } else if this.count - 1 == this.max_chained_neighbor_updates {
            log::error!(
                "Too many chained neighbor updates. Skipping the rest. First skipped position: {}",
                pos.to_short_string()
            );
        }
        if !running_already {
            Self::run_updates(host);
        }
    }

    fn run_updates<H: NeighborUpdateHost>(host: &mut H) {
        // Vanilla clears everything in a `finally`; the guard does the same when a block behavior unwinds.
        struct Reset<'a, H: NeighborUpdateHost>(&'a mut H);
        impl<H: NeighborUpdateHost> Drop for Reset<'_, H> {
            fn drop(&mut self) {
                let u = self.0.neighbor_updater();
                u.stack.clear();
                u.added_this_layer.clear();
                u.count = 0;
            }
        }
        let guard = Reset(host);
        let host = &mut *guard.0;
        loop {
            let u = host.neighbor_updater();
            if u.stack.is_empty() && u.added_this_layer.is_empty() {
                break;
            }
            while let Some(added) = u.added_this_layer.pop() {
                u.stack.push(added);
            }
            // `added_this_layer` was iterated back to front above and pushed in that order: the first added update
            // ends up on top, like vanilla.
            let Some(mut next) = u.stack.pop() else { break };
            loop {
                let more = next.run_next(host);
                if !more {
                    break;
                }
                if !host.neighbor_updater().added_this_layer.is_empty() {
                    host.neighbor_updater().stack.push(next);
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records executed updates; `chain` makes the update at a position enqueue another update (like a block whose
    /// `neighborChanged` changes a neighbor).
    struct Mock {
        updater: CollectingNeighborUpdater,
        log: Vec<String>,
        chain: Vec<(BlockPos, BlockPos)>,
        shape_chain: Vec<(BlockPos, BlockPos)>,
    }

    impl Mock {
        fn new(max: i32) -> Self {
            Mock {
                updater: CollectingNeighborUpdater::new(max),
                log: vec![],
                chain: vec![],
                shape_chain: vec![],
            }
        }
    }

    impl NeighborUpdateHost for Mock {
        fn neighbor_updater(&mut self) -> &mut CollectingNeighborUpdater {
            &mut self.updater
        }
        fn host_block_state(&self, _pos: BlockPos) -> BlockState {
            Block::STONE.default_state()
        }
        fn execute_shape_update(
            &mut self,
            direction: Direction,
            pos: BlockPos,
            _np: BlockPos,
            _ns: BlockState,
            _f: u32,
            _l: i32,
        ) {
            self.log.push(format!(
                "shape {} {}",
                pos.to_short_string(),
                direction.get_name()
            ));
        }
        fn execute_update(
            &mut self,
            _s: BlockState,
            pos: BlockPos,
            _b: Block,
            _o: Option<Orientation>,
            _p: bool,
        ) {
            self.log.push(format!("update {}", pos.to_short_string()));
            let targets: Vec<BlockPos> = self
                .chain
                .iter()
                .filter(|(from, _)| *from == pos)
                .map(|(_, to)| *to)
                .collect();
            for to in targets {
                CollectingNeighborUpdater::neighbor_changed(self, to, Block::STONE, None);
            }
            let shape_targets: Vec<BlockPos> = self
                .shape_chain
                .iter()
                .filter(|(from, _)| *from == pos)
                .map(|(_, to)| *to)
                .collect();
            for to in shape_targets {
                CollectingNeighborUpdater::shape_update(
                    self,
                    Direction::Up,
                    Block::STONE.default_state(),
                    to,
                    pos,
                    3,
                    512,
                );
            }
        }
        fn redstone_experiments(&self) -> bool {
            false
        }
        fn initial_orientation(
            &mut self,
            _f: Option<Direction>,
            _u: Option<Direction>,
        ) -> Option<Orientation> {
            None
        }
    }

    fn p(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    #[test]
    fn multi_update_runs_in_west_east_down_up_north_south_order() {
        let mut m = Mock::new(1_000_000);
        CollectingNeighborUpdater::update_neighbors_at_except_from_facing(
            &mut m,
            p(0, 0, 0),
            Block::STONE,
            None,
            None,
        );
        assert_eq!(
            m.log,
            [
                "update -1, 0, 0",
                "update 1, 0, 0",
                "update 0, -1, 0",
                "update 0, 1, 0",
                "update 0, 0, -1",
                "update 0, 0, 1"
            ]
        );
        assert_eq!(m.updater.count(), 0);
    }

    #[test]
    fn skipped_direction_is_not_updated() {
        for skip in Direction::ALL {
            let mut m = Mock::new(1_000_000);
            CollectingNeighborUpdater::update_neighbors_at_except_from_facing(
                &mut m,
                p(0, 0, 0),
                Block::STONE,
                Some(skip),
                None,
            );
            assert_eq!(m.log.len(), 5, "skip {skip:?}");
            assert!(!m.log.contains(&format!(
                "update {}",
                p(0, 0, 0).relative(skip).to_short_string()
            )));
        }
    }

    #[test]
    fn chained_updates_run_depth_first_before_remaining_directions() {
        let mut m = Mock::new(1_000_000);
        // the update at the west neighbor enqueues one at (50,0,0) and that one enqueues (60,0,0)
        m.chain = vec![(p(-1, 0, 0), p(50, 0, 0)), (p(50, 0, 0), p(60, 0, 0))];
        CollectingNeighborUpdater::update_neighbors_at_except_from_facing(
            &mut m,
            p(0, 0, 0),
            Block::STONE,
            None,
            None,
        );
        assert_eq!(
            m.log,
            [
                "update -1, 0, 0",
                "update 50, 0, 0",
                "update 60, 0, 0",
                "update 1, 0, 0",
                "update 0, -1, 0",
                "update 0, 1, 0",
                "update 0, 0, -1",
                "update 0, 0, 1"
            ]
        );
    }

    #[test]
    fn several_updates_added_by_one_update_run_in_insertion_order() {
        let mut m = Mock::new(1_000_000);
        m.chain = vec![
            (p(10, 0, 0), p(20, 0, 0)),
            (p(10, 0, 0), p(30, 0, 0)),
            (p(20, 0, 0), p(21, 0, 0)),
        ];
        CollectingNeighborUpdater::neighbor_changed(&mut m, p(10, 0, 0), Block::STONE, None);
        assert_eq!(
            m.log,
            [
                "update 10, 0, 0",
                "update 20, 0, 0",
                "update 21, 0, 0",
                "update 30, 0, 0"
            ]
        );
    }

    #[test]
    fn chain_limit_skips_the_rest() {
        let mut m = Mock::new(3);
        // an update that spawns 5 more: only 2 of them fit under the cap of 3 chained updates
        m.chain = (1..=5).map(|i| (p(0, 0, 0), p(i, 0, 0))).collect();
        CollectingNeighborUpdater::neighbor_changed(&mut m, p(0, 0, 0), Block::STONE, None);
        assert_eq!(
            m.log,
            ["update 0, 0, 0", "update 1, 0, 0", "update 2, 0, 0"]
        );
        assert_eq!(m.updater.count(), 0, "state is reset after the run");
        // the updater is usable again afterwards
        m.log.clear();
        CollectingNeighborUpdater::neighbor_changed(&mut m, p(7, 0, 0), Block::STONE, None);
        assert_eq!(m.log, ["update 7, 0, 0"]);
    }

    #[test]
    fn negative_limit_is_unlimited() {
        let mut m = Mock::new(-1);
        m.chain = (1..=50).map(|i| (p(0, 0, 0), p(i, 0, 0))).collect();
        CollectingNeighborUpdater::neighbor_changed(&mut m, p(0, 0, 0), Block::STONE, None);
        assert_eq!(m.log.len(), 51);
    }

    #[test]
    fn shape_updates_are_queued_like_neighbor_updates() {
        let mut m = Mock::new(1_000_000);
        m.shape_chain = vec![(p(0, 0, 0), p(5, 5, 5))];
        CollectingNeighborUpdater::neighbor_changed(&mut m, p(0, 0, 0), Block::STONE, None);
        assert_eq!(m.log, ["update 0, 0, 0", "shape 5, 5, 5 up"]);
    }
}
