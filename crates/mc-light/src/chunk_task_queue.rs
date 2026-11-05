//! `ChunkTaskPriorityQueue`: tasks grouped per chunk, bucketed by queue level (lowest level first, chunks of a level in
//! insertion order). Shared by the game's chunk dispatchers and [`crate::ThreadedLevelLightEngine`].

use indexmap::IndexMap;

pub struct ChunkTaskPriorityQueue<T> {
    /// `queuesPerPriority`: insertion-ordered chunk -> its tasks.
    queues: Vec<IndexMap<i64, Vec<T>>>,
    top_priority_queue_index: i32,
    /// `PRIORITY_LEVEL_COUNT` = `ChunkLevel.MAX_LEVEL + 2`.
    level_count: i32,
}

impl<T> ChunkTaskPriorityQueue<T> {
    pub fn new(level_count: i32) -> Self {
        Self { queues: (0..level_count).map(|_| IndexMap::new()).collect(), top_priority_queue_index: level_count, level_count }
    }

    fn advance_top(&mut self) {
        while self.has_work() && self.queues[self.top_priority_queue_index as usize].is_empty() {
            self.top_priority_queue_index += 1;
        }
    }

    /// `resortChunkTasks(oldPriority, pos, newPriority)`.
    pub fn resort_chunk_tasks(&mut self, old_priority: i32, pos: i64, new_priority: i32) {
        if old_priority < self.level_count {
            let old_tasks = self.queues[old_priority as usize].shift_remove(&pos);
            if old_priority == self.top_priority_queue_index {
                self.advance_top();
            }
            if let Some(old_tasks) = old_tasks {
                if !old_tasks.is_empty() {
                    self.queues[new_priority as usize].entry(pos).or_default().extend(old_tasks);
                    self.top_priority_queue_index = self.top_priority_queue_index.min(new_priority);
                }
            }
        }
    }

    pub fn submit(&mut self, task: T, chunk_pos: i64, level: i32) {
        self.queues[level as usize].entry(chunk_pos).or_default().push(task);
        self.top_priority_queue_index = self.top_priority_queue_index.min(level);
    }

    /// `release(pos, unschedule)`.
    pub fn release(&mut self, pos: i64, unschedule: bool) {
        for queue in &mut self.queues {
            if let Some(tasks) = queue.get_mut(&pos) {
                if unschedule {
                    tasks.clear();
                }
                if tasks.is_empty() {
                    queue.shift_remove(&pos);
                }
            }
        }
        self.advance_top();
    }

    /// `pop()`: the tasks of the first chunk of the best non-empty level.
    pub fn pop(&mut self) -> Option<(i64, Vec<T>)> {
        if !self.has_work() {
            return None;
        }
        let queue = &mut self.queues[self.top_priority_queue_index as usize];
        let (pos, tasks) = queue.shift_remove_index(0).expect("non-empty top queue");
        self.advance_top();
        Some((pos, tasks))
    }

    pub fn has_work(&self) -> bool {
        self.top_priority_queue_index < self.level_count
    }
}
