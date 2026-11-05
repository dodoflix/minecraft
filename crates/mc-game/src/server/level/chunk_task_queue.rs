//! `ChunkTaskPriorityQueue` (tasks grouped per chunk, bucketed by ticket level, lowest level first) and the
//! single-threaded form of `ChunkTaskDispatcher` / `ThrottlingChunkTaskDispatcher`.
//!
//! Vanilla runs the dispatcher's bookkeeping messages (level change > release > submit > poll) on a pool thread and
//! hands task bodies to an executor. Here the messages are queued and [`ChunkTaskDispatcher::process`] drains them in
//! the same priority order on the game thread; task bodies run through the caller-provided closure.

use super::fastutil::{LongOpenHashMap, LongOpenHashSet};
use super::chunk_level;
pub use mc_light::chunk_task_queue::ChunkTaskPriorityQueue;

/// `ChunkTaskPriorityQueue.PRIORITY_LEVEL_COUNT`.
pub fn priority_level_count() -> i32 {
    chunk_level::max_level() + 2
}

enum Message<T, R> {
    LevelChange { pos: i64, new_level: i32 },
    Release { pos: i64, clear_queue: bool, when_released: Option<R> },
    Submit { task: T, pos: i64, level: i32 },
}

/// `ChunkTaskDispatcher` with an optional `maxChunksInExecution` throttle (`ThrottlingChunkTaskDispatcher`).
///
/// `T` is a task body, `R` what to run once a [`release`](Self::release) was processed.
pub struct ChunkTaskDispatcher<T, R> {
    queue: ChunkTaskPriorityQueue<T>,
    /// The queue level each chunk currently has in `queue` (vanilla keeps it in the holder / the ticket tracker).
    queue_levels: LongOpenHashMap<i32>,
    messages: Vec<Message<T, R>>,
    sleeping: bool,
    max_chunks_in_execution: Option<usize>,
    /// Level reported for chunks without a recorded queue level (`defaultReturnValue`).
    default_queue_level: i32,
    chunk_positions_in_execution: LongOpenHashSet,
}

impl<T, R> ChunkTaskDispatcher<T, R> {
    pub fn new(max_chunks_in_execution: Option<usize>, default_queue_level: i32) -> Self {
        Self {
            queue: ChunkTaskPriorityQueue::new(priority_level_count()),
            queue_levels: LongOpenHashMap::new(),
            messages: Vec::new(),
            sleeping: true,
            max_chunks_in_execution,
            default_queue_level,
            chunk_positions_in_execution: LongOpenHashSet::new(),
        }
    }

    pub fn has_work(&self) -> bool {
        !self.messages.is_empty() || self.queue.has_work()
    }

    /// The queue level currently recorded for `pos` (`ChunkMap.getChunkQueueLevel` / `PlayerTicketTracker.queueLevels`).
    pub fn queue_level(&self, pos: i64) -> i32 {
        self.queue_levels.get(pos).copied().unwrap_or(self.default_queue_level)
    }

    /// `onLevelChange(pos, oldLevel, newLevel, setQueueLevel)`.
    pub fn on_level_change(&mut self, pos: i64, new_level: i32) {
        self.messages.push(Message::LevelChange { pos, new_level });
    }

    /// `release(pos, whenReleased, clearQueue)`.
    pub fn release(&mut self, pos: i64, when_released: Option<R>, clear_queue: bool) {
        self.messages.push(Message::Release { pos, clear_queue, when_released });
    }

    /// `submit(task, pos, level)`.
    pub fn submit(&mut self, task: T, pos: i64, level: i32) {
        self.messages.push(Message::Submit { task, pos, level });
    }

    /// Process every queued message in vanilla's priority order (level changes, releases, submits), then hand queued
    /// task bodies to `run` while the throttle allows. `released` receives each processed release action.
    pub fn process(&mut self, mut run: impl FnMut(i64, T), mut released: impl FnMut(R)) {
        loop {
            let messages = std::mem::take(&mut self.messages);
            if messages.is_empty() {
                break;
            }
            let (mut level_changes, mut releases, mut submits) = (Vec::new(), Vec::new(), Vec::new());
            for message in messages {
                match message {
                    Message::LevelChange { .. } => level_changes.push(message),
                    Message::Release { .. } => releases.push(message),
                    Message::Submit { .. } => submits.push(message),
                }
            }
            for message in level_changes {
                if let Message::LevelChange { pos, new_level } = message {
                    let old = self.queue_level(pos);
                    self.queue.resort_chunk_tasks(old, pos, new_level);
                    if new_level >= self.default_queue_level {
                        self.queue_levels.remove(pos);
                    } else {
                        self.queue_levels.insert(pos, new_level);
                    }
                }
            }
            for message in releases {
                if let Message::Release { pos, clear_queue, when_released } = message {
                    self.queue.release(pos, clear_queue);
                    self.chunk_positions_in_execution.remove(pos);
                    self.sleeping = false;
                    if let Some(r) = when_released {
                        released(r);
                    }
                }
            }
            for message in submits {
                if let Message::Submit { task, pos, level } = message {
                    self.queue.submit(task, pos, level);
                    self.sleeping = false;
                }
            }
            self.poll(&mut run);
        }
        // Pending releases may have freed throttle slots after the last poll.
        if !self.sleeping {
            self.poll(&mut run);
        }
    }

    fn poll(&mut self, run: &mut impl FnMut(i64, T)) {
        loop {
            let throttled = self.max_chunks_in_execution.is_some_and(|max| self.chunk_positions_in_execution.len() >= max);
            let popped = if throttled { None } else { self.queue.pop() };
            match popped {
                None => {
                    self.sleeping = true;
                    return;
                }
                Some((pos, tasks)) => {
                    if self.max_chunks_in_execution.is_some() {
                        self.chunk_positions_in_execution.add(pos);
                    }
                    for task in tasks {
                        run(pos, task);
                    }
                }
            }
        }
    }

    /// `getDebugStatus()` of the throttling dispatcher.
    pub fn debug_status(&self, name: &str) -> String {
        let positions: Vec<String> =
            self.chunk_positions_in_execution.iter().map(|k| format!("{k}:{}", mc_core::pos::ChunkPos::unpack(k))).collect();
        format!("{name}=[{}], s={}", positions.join(","), self.sleeping)
    }
}
