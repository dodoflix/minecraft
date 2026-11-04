//! `MinecraftServer`: owned by the game thread. Skeleton owned by the lifecycle slice (tick loop, task queue,
//! level container). Other slices add their state as typed fields (each type lives in its own module).

use super::shared::{ServerShared, ServerTask};
use crate::server::level::ServerLevel;
use crate::world::level::DimensionKey;
use indexmap::IndexMap;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

pub struct MinecraftServer {
    pub shared: Arc<ServerShared>,
    tasks: Receiver<ServerTask>,
    /// `levels` in vanilla iteration order. A level is `None` only while it is taken out for its own tick
    /// (see [`MinecraftServer::with_level`]).
    levels: IndexMap<DimensionKey, Option<Box<ServerLevel>>>,
    pub tick_count: i32,
}

impl MinecraftServer {
    pub fn new(shared: Arc<ServerShared>, tasks: Receiver<ServerTask>) -> Self {
        Self { shared, tasks, levels: IndexMap::new(), tick_count: 0 }
    }

    /// Run everything queued with `ServerShared::execute` (`runAllTasks`).
    pub fn run_all_tasks(&mut self) {
        while let Ok(task) = self.tasks.try_recv() {
            task(self);
        }
    }

    pub fn add_level(&mut self, level: ServerLevel) {
        self.levels.insert(level.dimension.clone(), Some(Box::new(level)));
    }

    pub fn level(&self, dimension: &DimensionKey) -> Option<&ServerLevel> {
        self.levels.get(dimension).and_then(|l| l.as_deref())
    }

    pub fn level_mut(&mut self, dimension: &DimensionKey) -> Option<&mut ServerLevel> {
        self.levels.get_mut(dimension).and_then(|l| l.as_deref_mut())
    }

    /// Dimension keys in vanilla order (overworld first).
    pub fn level_keys(&self) -> Vec<DimensionKey> {
        self.levels.keys().cloned().collect()
    }

    /// Take `dimension` out of the level map, run `f` with `&mut MinecraftServer` (that level absent: queries for it
    /// return `None`) and `&mut ServerLevel`, then put it back. This is how a level ticks while still being able to
    /// reach the server and the other levels. Returns `None` if the level doesn't exist or is already taken out.
    pub fn with_level<R>(&mut self, dimension: &DimensionKey, f: impl FnOnce(&mut MinecraftServer, &mut ServerLevel) -> R) -> Option<R> {
        let mut level = self.levels.get_mut(dimension)?.take()?;
        let result = f(self, &mut level);
        if let Some(slot) = self.levels.get_mut(dimension) {
            *slot = Some(level);
        }
        Some(result)
    }
}
