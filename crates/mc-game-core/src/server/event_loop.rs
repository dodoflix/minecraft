//! `BlockableEventLoop` / `ReentrantBlockableEventLoop` as used by `MinecraftServer`: the pending task queue
//! (`TickTask`s submitted through [`ServerShared::execute`]), the blocking depth and the reentrancy depth.
//! The task-running methods themselves live on `MinecraftServer` (tasks receive `&mut MinecraftServer`).

use super::shared::TickTask;
use std::collections::VecDeque;
use std::sync::mpsc::Receiver;

pub struct TaskQueue {
    rx: Receiver<TickTask>,
    /// Drained from `rx` in submission order; vanilla's `pendingRunnables` (`peek` needs a local buffer).
    pending: VecDeque<TickTask>,
    /// `BlockableEventLoop.blockingCount` (`managedBlock` nesting).
    pub blocking_count: u32,
    /// `ReentrantBlockableEventLoop.reentrantCount` (tasks currently running).
    pub reentrant_count: u32,
}

impl TaskQueue {
    pub fn new(rx: Receiver<TickTask>) -> Self {
        Self { rx, pending: VecDeque::new(), blocking_count: 0, reentrant_count: 0 }
    }

    fn refill(&mut self) {
        while let Ok(task) = self.rx.try_recv() {
            self.pending.push_back(task);
        }
    }

    /// `pendingRunnables.peek()`.
    pub fn peek(&mut self) -> Option<&TickTask> {
        self.refill();
        self.pending.front()
    }

    /// `pendingRunnables.remove()`.
    pub fn pop(&mut self) -> Option<TickTask> {
        self.refill();
        self.pending.pop_front()
    }

    /// `getPendingTasksCount`.
    pub fn len(&mut self) -> usize {
        self.refill();
        self.pending.len()
    }

    pub fn is_empty(&mut self) -> bool {
        self.len() == 0
    }

    /// `dropAllTasks`.
    pub fn clear(&mut self) {
        self.refill();
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn fifo_with_peek() {
        let (tx, rx) = channel();
        let mut q = TaskQueue::new(rx);
        assert!(q.peek().is_none());
        for tick in [1, 2, 3] {
            tx.send(TickTask { tick, run: Box::new(|_| {}) }).unwrap();
        }
        assert_eq!(q.peek().map(|t| t.tick), Some(1));
        assert_eq!(q.len(), 3);
        assert_eq!(q.pop().map(|t| t.tick), Some(1));
        tx.send(TickTask { tick: 4, run: Box::new(|_| {}) }).unwrap();
        assert_eq!(q.pop().map(|t| t.tick), Some(2));
        assert_eq!(q.pop().map(|t| t.tick), Some(3));
        assert_eq!(q.pop().map(|t| t.tick), Some(4));
        assert!(q.is_empty());
    }
}
