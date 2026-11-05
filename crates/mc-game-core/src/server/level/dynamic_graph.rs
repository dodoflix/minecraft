//! `DynamicGraphMinFixedPoint` + `LeveledPriorityQueue` + `ChunkTracker`: the incremental "minimum over neighbours + 1"
//! solver behind chunk ticket levels (`LoadingChunkTracker`, `SimulationChunkTracker`, the player distance trackers).
//!
//! Vanilla's abstract class is split into the solver state ([`GraphState`], all the non-abstract methods) and the
//! behaviour ([`GraphBehavior`], the abstract methods) which owns whatever the level lookups need.

use super::fastutil::LongOpenHashMap;
use mc_core::math::mth;
use mc_core::pos::ChunkPos;
use std::collections::VecDeque;

/// `LeveledPriorityQueue`: one FIFO set per level (insertion ordered, no duplicates).
struct LeveledPriorityQueue {
    level_count: i32,
    queues: Vec<LinkedLongSet>,
    first_queued_level: i32,
}

/// `LongLinkedOpenHashSet`: insertion-ordered set (removal keeps the order of the rest).
#[derive(Default)]
struct LinkedLongSet {
    order: VecDeque<i64>,
    members: std::collections::HashSet<i64>,
    /// Lazily removed entries still present in `order`.
    stale: std::collections::HashMap<i64, u32>,
}

impl LinkedLongSet {
    fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
    fn add(&mut self, k: i64) {
        if self.members.insert(k) {
            self.order.push_back(k);
        }
    }
    fn remove(&mut self, k: i64) {
        if self.members.remove(&k) {
            *self.stale.entry(k).or_insert(0) += 1;
        }
    }
    fn remove_first(&mut self) -> i64 {
        loop {
            let k = self.order.pop_front().expect("queue not empty");
            if let Some(n) = self.stale.get_mut(&k) {
                *n -= 1;
                if *n == 0 {
                    self.stale.remove(&k);
                }
                continue;
            }
            self.members.remove(&k);
            return k;
        }
    }
}

impl LeveledPriorityQueue {
    fn new(level_count: i32) -> Self {
        Self { level_count, queues: (0..level_count).map(|_| LinkedLongSet::default()).collect(), first_queued_level: level_count }
    }

    fn remove_first_long(&mut self) -> i64 {
        let queue = &mut self.queues[self.first_queued_level as usize];
        let result = queue.remove_first();
        if queue.is_empty() {
            self.check_first_queued_level(self.level_count);
        }
        result
    }

    fn is_empty(&self) -> bool {
        self.first_queued_level >= self.level_count
    }

    fn dequeue(&mut self, node: i64, key: i32, upper_bound: i32) {
        let queue = &mut self.queues[key as usize];
        queue.remove(node);
        if queue.is_empty() && self.first_queued_level == key {
            self.check_first_queued_level(upper_bound);
        }
    }

    fn enqueue(&mut self, node: i64, key: i32) {
        self.queues[key as usize].add(node);
        if self.first_queued_level > key {
            self.first_queued_level = key;
        }
    }

    fn check_first_queued_level(&mut self, upper_bound: i32) {
        let old_level = self.first_queued_level;
        self.first_queued_level = upper_bound;
        for i in (old_level + 1)..upper_bound {
            if !self.queues[i as usize].is_empty() {
                self.first_queued_level = i;
                break;
            }
        }
    }
}

const NO_COMPUTED_LEVEL: i32 = 255;

/// The abstract methods of `DynamicGraphMinFixedPoint`.
pub trait GraphBehavior {
    fn is_source(&self, node: i64) -> bool;
    fn get_computed_level(&mut self, st: &GraphState, node: i64, known_parent: i64, known_level_from_parent: i32) -> i32;
    fn check_neighbors_after_update(&mut self, st: &mut GraphState, node: i64, level: i32, only_decrease: bool);
    fn get_level(&mut self, node: i64) -> i32;
    fn set_level(&mut self, node: i64, level: i32);
    fn compute_level_from_neighbor(&mut self, from: i64, to: i64, from_level: i32) -> i32;
}

/// Solver state of one `DynamicGraphMinFixedPoint`.
pub struct GraphState {
    pub level_count: i32,
    priority_queue: LeveledPriorityQueue,
    computed_levels: LongOpenHashMap<u8>,
    has_work: bool,
}

impl GraphState {
    pub fn new(level_count: i32) -> Self {
        assert!(level_count < 254, "Level count must be < 254.");
        Self { level_count, priority_queue: LeveledPriorityQueue::new(level_count), computed_levels: LongOpenHashMap::new(), has_work: false }
    }

    fn computed(&self, node: i64) -> i32 {
        self.computed_levels.get(node).map_or(NO_COMPUTED_LEVEL, |&v| i32::from(v))
    }

    pub fn has_work(&self) -> bool {
        self.has_work
    }

    fn calculate_priority(&self, level: i32, computed_level: i32) -> i32 {
        level.min(computed_level).min(self.level_count - 1)
    }

    /// `removeFromQueue(node)`.
    pub fn remove_from_queue<B: GraphBehavior>(&mut self, b: &mut B, node: i64) {
        let computed_level = self.computed_levels.remove(node).map_or(NO_COMPUTED_LEVEL, i32::from);
        if computed_level != NO_COMPUTED_LEVEL {
            let level = b.get_level(node);
            let priority = self.calculate_priority(level, computed_level);
            self.priority_queue.dequeue(node, priority, self.level_count);
            self.has_work = !self.priority_queue.is_empty();
        }
    }

    pub fn check_node<B: GraphBehavior>(&mut self, b: &mut B, node: i64) {
        self.check_edge(b, node, node, self.level_count - 1, false);
    }

    /// `checkEdge(from, to, newLevelFrom, onlyDecreased)`.
    pub fn check_edge<B: GraphBehavior>(&mut self, b: &mut B, from: i64, to: i64, new_level_from: i32, only_decreased: bool) {
        let level_to = b.get_level(to);
        let old_computed = self.computed(to);
        self.check_edge_full(b, from, to, new_level_from, level_to, old_computed, only_decreased);
        self.has_work = !self.priority_queue.is_empty();
    }

    #[allow(clippy::too_many_arguments)]
    fn check_edge_full<B: GraphBehavior>(
        &mut self,
        b: &mut B,
        from: i64,
        to: i64,
        mut new_level_from: i32,
        mut level_to: i32,
        mut old_computed_level: i32,
        only_decreased: bool,
    ) {
        if b.is_source(to) {
            return;
        }
        new_level_from = mth::clamp_i32(new_level_from, 0, self.level_count - 1);
        level_to = mth::clamp_i32(level_to, 0, self.level_count - 1);
        let was_consistent = old_computed_level == NO_COMPUTED_LEVEL;
        if was_consistent {
            old_computed_level = level_to;
        }
        let new_computed_level = if only_decreased {
            old_computed_level.min(new_level_from)
        } else {
            mth::clamp_i32(b.get_computed_level(self, to, from, new_level_from), 0, self.level_count - 1)
        };
        let old_priority = self.calculate_priority(level_to, old_computed_level);
        if level_to != new_computed_level {
            let new_priority = self.calculate_priority(level_to, new_computed_level);
            if old_priority != new_priority && !was_consistent {
                self.priority_queue.dequeue(to, old_priority, new_priority);
            }
            self.priority_queue.enqueue(to, new_priority);
            self.computed_levels.insert(to, new_computed_level as u8);
        } else if !was_consistent {
            self.priority_queue.dequeue(to, old_priority, self.level_count);
            self.computed_levels.remove(to);
        }
    }

    /// `checkNeighbor(from, to, level, onlyDecreased)`.
    pub fn check_neighbor<B: GraphBehavior>(&mut self, b: &mut B, from: i64, to: i64, level: i32, only_decreased: bool) {
        let stored_old_computed_level = self.computed(to);
        let level_from = mth::clamp_i32(b.compute_level_from_neighbor(from, to, level), 0, self.level_count - 1);
        if only_decreased {
            let level_to = b.get_level(to);
            self.check_edge_full(b, from, to, level_from, level_to, stored_old_computed_level, only_decreased);
        } else {
            let was_consistent = stored_old_computed_level == NO_COMPUTED_LEVEL;
            let old_computed_level = if was_consistent {
                mth::clamp_i32(b.get_level(to), 0, self.level_count - 1)
            } else {
                stored_old_computed_level
            };
            if level_from == old_computed_level {
                let level_to = if was_consistent { old_computed_level } else { b.get_level(to) };
                self.check_edge_full(b, from, to, self.level_count - 1, level_to, stored_old_computed_level, only_decreased);
            }
        }
    }

    /// `runUpdates(count)`: returns the unused part of `count`.
    pub fn run_updates<B: GraphBehavior>(&mut self, b: &mut B, mut count: i32) -> i32 {
        if self.priority_queue.is_empty() {
            return count;
        }
        while !self.priority_queue.is_empty() && count > 0 {
            count -= 1;
            let node = self.priority_queue.remove_first_long();
            let level = mth::clamp_i32(b.get_level(node), 0, self.level_count - 1);
            let computed_level = self.computed_levels.remove(node).map_or(NO_COMPUTED_LEVEL, i32::from);
            if computed_level < level {
                b.set_level(node, computed_level);
                b.check_neighbors_after_update(self, node, computed_level, true);
            } else if computed_level > level {
                b.set_level(node, self.level_count - 1);
                if computed_level != self.level_count - 1 {
                    let priority = self.calculate_priority(self.level_count - 1, computed_level);
                    self.priority_queue.enqueue(node, priority);
                    self.computed_levels.insert(node, computed_level as u8);
                }
                b.check_neighbors_after_update(self, node, level, false);
            }
        }
        self.has_work = !self.priority_queue.is_empty();
        count
    }

    pub fn get_queue_size(&self) -> usize {
        self.computed_levels.len()
    }
}

/// `ChunkTracker`'s abstract part: subclasses supply levels; the 3x3 neighbourhood logic is shared.
pub trait ChunkTrackerBehavior {
    fn get_level(&mut self, node: i64) -> i32;
    fn set_level(&mut self, node: i64, level: i32);
    fn get_level_from_source(&mut self, to: i64) -> i32;
}

/// `ChunkTracker`: implements [`GraphBehavior`] for a [`ChunkTrackerBehavior`].
pub struct ChunkTracker<'a, T: ChunkTrackerBehavior>(pub &'a mut T);

impl<T: ChunkTrackerBehavior> GraphBehavior for ChunkTracker<'_, T> {
    fn is_source(&self, node: i64) -> bool {
        node == ChunkPos::INVALID_CHUNK_POS
    }

    fn check_neighbors_after_update(&mut self, st: &mut GraphState, node: i64, level: i32, only_decrease: bool) {
        if !only_decrease || level < st.level_count - 2 {
            let pos = ChunkPos::unpack(node);
            let (x, z) = (pos.x, pos.z);
            for offset_x in -1..=1 {
                for offset_z in -1..=1 {
                    let neighbor = ChunkPos::pack_xz(x.wrapping_add(offset_x), z.wrapping_add(offset_z));
                    if neighbor != node {
                        st.check_neighbor(self, node, neighbor, level, only_decrease);
                    }
                }
            }
        }
    }

    fn get_computed_level(&mut self, _st: &GraphState, node: i64, known_parent: i64, known_level_from_parent: i32) -> i32 {
        let mut computed_level = known_level_from_parent;
        let pos = ChunkPos::unpack(node);
        let (x, z) = (pos.x, pos.z);
        for offset_x in -1..=1 {
            for offset_z in -1..=1 {
                let mut neighbor = ChunkPos::pack_xz(x.wrapping_add(offset_x), z.wrapping_add(offset_z));
                if neighbor == node {
                    neighbor = ChunkPos::INVALID_CHUNK_POS;
                }
                if neighbor != known_parent {
                    let level = self.0.get_level(neighbor);
                    let cost_from_neighbor = self.compute_level_from_neighbor(neighbor, node, level);
                    if computed_level > cost_from_neighbor {
                        computed_level = cost_from_neighbor;
                    }
                    if computed_level == 0 {
                        return computed_level;
                    }
                }
            }
        }
        computed_level
    }

    fn get_level(&mut self, node: i64) -> i32 {
        self.0.get_level(node)
    }

    fn set_level(&mut self, node: i64, level: i32) {
        self.0.set_level(node, level)
    }

    fn compute_level_from_neighbor(&mut self, from: i64, to: i64, from_level: i32) -> i32 {
        if from == ChunkPos::INVALID_CHUNK_POS { self.0.get_level_from_source(to) } else { from_level + 1 }
    }
}

impl GraphState {
    /// `ChunkTracker.update(node, newLevelFrom, onlyDecreased)`.
    pub fn chunk_tracker_update<T: ChunkTrackerBehavior>(&mut self, tracker: &mut T, node: i64, new_level_from: i32, only_decreased: bool) {
        self.check_edge(&mut ChunkTracker(tracker), ChunkPos::INVALID_CHUNK_POS, node, new_level_from, only_decreased);
    }

    pub fn run_chunk_tracker_updates<T: ChunkTrackerBehavior>(&mut self, tracker: &mut T, count: i32) -> i32 {
        self.run_updates(&mut ChunkTracker(tracker), count)
    }
}
