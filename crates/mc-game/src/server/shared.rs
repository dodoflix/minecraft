//! `ServerShared`: the thread-safe part of the server, handed out as `Arc<ServerShared>` to network tasks, chunk
//! worker threads and (through `ServerLevel::shared()`) to level code. Everything else about the server is owned by
//! the game thread (`MinecraftServer`) as plain `&mut` data. Add fields here only when something off the game thread
//! (or code that cannot reach `&mut MinecraftServer`) must read them; they must be `Send + Sync` (atomics, locks,
//! immutable data).

use super::MinecraftServer;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

/// A unit of work run on the game thread between ticks (`MinecraftServer.execute`/`executeIfPossible`).
pub type ServerTask = Box<dyn FnOnce(&mut MinecraftServer) + Send>;

pub struct ServerShared {
    /// `MinecraftServer.running` (cleared by `halt`; polled by the tick loop).
    running: AtomicBool,
    /// `MinecraftServer.tickCount`, mirrored for readers off the game thread (status, watchdog).
    tick_count: AtomicI32,
    task_tx: Mutex<Sender<ServerTask>>,
}

impl ServerShared {
    pub fn new() -> (Arc<Self>, Receiver<ServerTask>) {
        let (tx, rx) = channel();
        let shared = Arc::new(Self { running: AtomicBool::new(true), tick_count: AtomicI32::new(0), task_tx: Mutex::new(tx) });
        (shared, rx)
    }
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
    /// `halt(false)`: ask the server to stop after the current tick.
    pub fn halt(&self) {
        self.running.store(false, Ordering::Release);
    }
    pub fn tick_count(&self) -> i32 {
        self.tick_count.load(Ordering::Relaxed)
    }
    #[allow(dead_code)]
    pub(crate) fn set_tick_count(&self, n: i32) {
        self.tick_count.store(n, Ordering::Relaxed);
    }
    /// `MinecraftServer.execute(task)`: run `task` on the game thread (next task drain). Safe from any thread.
    pub fn execute(&self, task: impl FnOnce(&mut MinecraftServer) + Send + 'static) {
        // A send error means the game thread is gone (server stopped): dropping the task matches vanilla.
        let _ = self.task_tx.lock().unwrap_or_else(|e| e.into_inner()).send(Box::new(task));
    }
}
